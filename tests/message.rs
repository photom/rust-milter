//! Scenarios from `docs/testing/message.md`.

use proptest::prelude::*;
use rust_milter::protocol::*;
use rust_milter::{
    AddressFamily, Bytes, Command, Error, Frame, MilterText, ProtocolError, Reply, ReplyCode,
    ReplyError,
};

fn frame(command: u8, data: &[u8]) -> Frame {
    Frame::new(command, Bytes::copy_from_slice(data))
}

/// Concatenates the 4-byte big-endian forms of `values`.
fn be32(values: &[u32]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for value in values {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}

fn assert_malformed(result: rust_milter::Result<Command>, command: u8) {
    assert!(
        matches!(
            result,
            Err(Error::Protocol(ProtocolError::Malformed { command: c })) if c == command
        ),
        "got {result:?}"
    );
}

// Command decoding: option negotiation

#[test]
fn cmd_001_decodes_option_negotiation() {
    let command = Command::from_frame(frame(SMFIC_OPTNEG, &be32(&[6, 0x1FF, 0x1F_FFFF])));

    assert_eq!(
        command.unwrap(),
        Command::OptNeg {
            version: 6,
            actions: 0x1FF,
            protocol: 0x1F_FFFF
        }
    );
}

#[test]
fn cmd_002_rejects_a_short_option_negotiation() {
    let data = &be32(&[6, 0x1FF, 0x1F_FFFF])[..11];

    assert_malformed(Command::from_frame(frame(SMFIC_OPTNEG, data)), SMFIC_OPTNEG);
}

#[test]
fn cmd_003_keeps_zero_actions_and_protocol_flags() {
    let command = Command::from_frame(frame(SMFIC_OPTNEG, &be32(&[2, 0, 0])));

    assert_eq!(
        command.unwrap(),
        Command::OptNeg {
            version: 2,
            actions: 0,
            protocol: 0
        }
    );
}

/// The raw bytes of each text, for comparing `MilterText` lists.
fn texts(values: &[MilterText]) -> Vec<&[u8]> {
    let mut texts = Vec::new();
    for value in values {
        texts.push(value.as_bytes());
    }
    texts
}

/// The raw bytes of an optional text.
fn bytes_of(value: &Option<MilterText>) -> Option<&[u8]> {
    match value {
        Some(text) => Some(text.as_bytes()),
        None => None,
    }
}

// Command decoding: macros

#[test]
fn cmd_010_decodes_macros_in_order() {
    let data = b"C{j}\0mx.void.invalid\0{daemon_name}\0smtpd\0";

    let Command::Macro { stage, macros } = Command::from_frame(frame(SMFIC_MACRO, data)).unwrap()
    else {
        panic!("not a Macro command");
    };

    assert_eq!(stage, b'C');
    let mut pairs: Vec<(&[u8], &[u8])> = Vec::new();
    for (name, value) in &macros {
        pairs.push((name.as_bytes(), value.as_bytes()));
    }
    assert_eq!(
        pairs,
        vec![
            (&b"{j}"[..], &b"mx.void.invalid"[..]),
            (&b"{daemon_name}"[..], &b"smtpd"[..])
        ]
    );
}

#[test]
fn cmd_011_keeps_postfix_stage_bytes() {
    for stage_byte in [b'L', b'B', b'U'] {
        let data = [&[stage_byte][..], b"{i}\0ABC123\0"].concat();

        let command = Command::from_frame(frame(SMFIC_MACRO, &data)).unwrap();

        assert!(
            matches!(command, Command::Macro { stage, .. } if stage == stage_byte),
            "stage {}: got {command:?}",
            char::from(stage_byte)
        );
    }
}

#[test]
fn cmd_012_rejects_a_macro_name_without_a_value() {
    assert_malformed(
        Command::from_frame(frame(SMFIC_MACRO, b"C{j}\0")),
        SMFIC_MACRO,
    );
}

// Command decoding: connection information

/// A `C` frame's data: hostname, family, then port and address when given.
fn connect_data(hostname: &[u8], family: u8, port_and_address: Option<(u16, &[u8])>) -> Vec<u8> {
    let mut data = [hostname, b"\0", &[family]].concat();
    if let Some((port, address)) = port_and_address {
        data.extend_from_slice(&port.to_be_bytes());
        data.extend_from_slice(address);
        data.push(0);
    }
    data
}

#[test]
fn cmd_020_decodes_an_ipv4_connection() {
    let data = connect_data(b"mail.void.invalid", SMFIA_INET, Some((25, b"192.0.2.1")));

    let command = Command::from_frame(frame(SMFIC_CONNECT, &data)).unwrap();

    let Command::Connect {
        hostname,
        family,
        port,
        address,
    } = command
    else {
        panic!("not a Connect command: {command:?}");
    };
    assert_eq!(hostname.as_bytes(), b"mail.void.invalid");
    assert_eq!(family, AddressFamily::Inet);
    assert_eq!(port, Some(25));
    assert_eq!(bytes_of(&address), Some(&b"192.0.2.1"[..]));
}

#[test]
fn cmd_021_decodes_an_ipv6_connection() {
    let data = connect_data(
        b"mail.void.invalid",
        SMFIA_INET6,
        Some((25, b"2001:db8::1")),
    );

    let command = Command::from_frame(frame(SMFIC_CONNECT, &data)).unwrap();

    let Command::Connect {
        family, address, ..
    } = command
    else {
        panic!("not a Connect command: {command:?}");
    };
    assert_eq!(family, AddressFamily::Inet6);
    assert_eq!(bytes_of(&address), Some(&b"2001:db8::1"[..]));
}

#[test]
fn cmd_022_decodes_an_unknown_family_without_port_or_address() {
    let data = connect_data(b"unknown", SMFIA_UNKNOWN, None);

    let command = Command::from_frame(frame(SMFIC_CONNECT, &data)).unwrap();

    let Command::Connect {
        family,
        port,
        address,
        ..
    } = command
    else {
        panic!("not a Connect command: {command:?}");
    };
    assert_eq!(family, AddressFamily::Unknown);
    assert_eq!(port, None);
    assert_eq!(address, None);
}

#[test]
fn cmd_023_decodes_a_local_client() {
    let data = connect_data(b"localhost", SMFIA_UNIX, Some((0, b"/run/smtp.sock")));

    let command = Command::from_frame(frame(SMFIC_CONNECT, &data)).unwrap();

    let Command::Connect {
        family,
        port,
        address,
        ..
    } = command
    else {
        panic!("not a Connect command: {command:?}");
    };
    assert_eq!(family, AddressFamily::Unix);
    assert_eq!(port, Some(0));
    assert_eq!(bytes_of(&address), Some(&b"/run/smtp.sock"[..]));
}

#[test]
fn cmd_024_rejects_an_ipv4_connection_without_port_and_address() {
    let data = connect_data(b"mail.void.invalid", SMFIA_INET, None);

    assert_malformed(
        Command::from_frame(frame(SMFIC_CONNECT, &data)),
        SMFIC_CONNECT,
    );
}

#[test]
fn cmd_025_rejects_an_unknown_family_byte() {
    let data = connect_data(b"mail.void.invalid", b'X', Some((25, b"192.0.2.1")));

    assert_malformed(
        Command::from_frame(frame(SMFIC_CONNECT, &data)),
        SMFIC_CONNECT,
    );
}

// Command decoding: SMTP commands, headers, and body

#[test]
fn cmd_030_decodes_helo() {
    let command = Command::from_frame(frame(SMFIC_HELO, b"client.void.invalid\0")).unwrap();

    let Command::Helo(name) = command else {
        panic!("not a Helo command: {command:?}");
    };
    assert_eq!(name.as_bytes(), b"client.void.invalid");
}

#[test]
fn cmd_031_decodes_mail_with_esmtp_arguments() {
    let data = b"<a@void.invalid>\0SIZE=100\0BODY=8BITMIME\0";

    let command = Command::from_frame(frame(SMFIC_MAIL, data)).unwrap();

    let Command::Mail(args) = command else {
        panic!("not a Mail command: {command:?}");
    };
    assert_eq!(
        texts(&args),
        vec![&b"<a@void.invalid>"[..], b"SIZE=100", b"BODY=8BITMIME"]
    );
}

#[test]
fn cmd_032_decodes_rcpt_without_arguments() {
    let command = Command::from_frame(frame(SMFIC_RCPT, b"<b@void.invalid>\0")).unwrap();

    let Command::Rcpt(args) = command else {
        panic!("not a Rcpt command: {command:?}");
    };
    assert_eq!(texts(&args), vec![&b"<b@void.invalid>"[..]]);
}

#[test]
fn cmd_033_keeps_a_folded_header_value_unchanged() {
    let data = b"Subject\0first line\n\tsecond line\0";

    let command = Command::from_frame(frame(SMFIC_HEADER, data)).unwrap();

    let Command::Header { name, value } = command else {
        panic!("not a Header command: {command:?}");
    };
    assert_eq!(name.as_bytes(), b"Subject");
    assert_eq!(value.as_bytes(), b"first line\n\tsecond line");
}

#[test]
fn cmd_034_keeps_binary_body_data_unchanged() {
    let data = b"line one\r\n\0binary\rbytes\r\n";

    let command = Command::from_frame(frame(SMFIC_BODY, data)).unwrap();

    assert_eq!(command, Command::Body(Bytes::from_static(data)));
}

#[test]
fn cmd_035_decodes_end_of_body_with_and_without_data() {
    let empty = Command::from_frame(frame(SMFIC_BODYEOB, b"")).unwrap();
    let with_data = Command::from_frame(frame(SMFIC_BODYEOB, b"0123456789")).unwrap();

    assert_eq!(empty, Command::BodyEob(Bytes::new()));
    assert_eq!(
        with_data,
        Command::BodyEob(Bytes::from_static(b"0123456789"))
    );
}

#[test]
fn cmd_036_decodes_the_commands_without_data() {
    let cases = [
        (SMFIC_ABORT, Command::Abort),
        (SMFIC_QUIT, Command::Quit),
        (SMFIC_QUIT_NC, Command::QuitNewConnection),
        (SMFIC_DATA, Command::Data),
        (SMFIC_EOH, Command::Eoh),
    ];

    for (code, expected) in cases {
        assert_eq!(Command::from_frame(frame(code, b"")).unwrap(), expected);
    }
}

#[test]
fn cmd_037_decodes_an_unknown_smtp_command() {
    let command = Command::from_frame(frame(SMFIC_UNKNOWN, b"XFOO bar\0")).unwrap();

    let Command::Unknown(text) = command else {
        panic!("not an Unknown command: {command:?}");
    };
    assert_eq!(text.as_bytes(), b"XFOO bar");
}

// Command decoding: errors and text

#[test]
fn cmd_040_rejects_an_unknown_command_code() {
    let result = Command::from_frame(frame(b'Z', b""));

    assert!(
        matches!(
            result,
            Err(Error::Protocol(ProtocolError::UnknownCommand(b'Z')))
        ),
        "got {result:?}"
    );
}

#[test]
fn cmd_041_rejects_a_string_without_a_null_terminator() {
    assert_malformed(
        Command::from_frame(frame(SMFIC_HELO, b"client.void.invalid")),
        SMFIC_HELO,
    );
}

#[test]
fn cmd_042_keeps_a_sender_that_is_not_utf8() {
    let data = b"<\xff\xfeuser@void.invalid>\0";

    let command = Command::from_frame(frame(SMFIC_MAIL, data)).unwrap();

    let Command::Mail(args) = command else {
        panic!("not a Mail command: {command:?}");
    };
    assert_eq!(args[0].as_bytes(), b"<\xff\xfeuser@void.invalid>");
    assert_eq!(args[0].as_str(), None);
}

#[test]
fn cmd_043_reads_an_smtputf8_sender_as_text() {
    let data = "<ユーザー@void.invalid>\0".as_bytes();

    let command = Command::from_frame(frame(SMFIC_MAIL, data)).unwrap();

    let Command::Mail(args) = command else {
        panic!("not a Mail command: {command:?}");
    };
    assert_eq!(args[0].as_str(), Some("<ユーザー@void.invalid>"));
}

// Reply encoding

fn encode(reply: Reply) -> Frame {
    reply.to_frame().expect("the reply is valid")
}

fn assert_invalid(reply: Reply) {
    let result = reply.clone().to_frame();
    assert!(
        matches!(result, Err(Error::InvalidReply(_))),
        "{reply:?}: got {result:?}"
    );
}

#[test]
fn rpl_001_encodes_the_replies_without_data() {
    let cases = [
        (Reply::Continue, SMFIR_CONTINUE),
        (Reply::Accept, SMFIR_ACCEPT),
        (Reply::Reject, SMFIR_REJECT),
        (Reply::TempFail, SMFIR_TEMPFAIL),
        (Reply::Discard, SMFIR_DISCARD),
        (Reply::Skip, SMFIR_SKIP),
        (Reply::Progress, SMFIR_PROGRESS),
    ];

    for (reply, code) in cases {
        assert_eq!(encode(reply), frame(code, b""));
    }
}

#[test]
fn rpl_002_encodes_option_negotiation_in_12_bytes() {
    let reply = Reply::OptNeg {
        version: 6,
        actions: 0x01,
        protocol: 0x00,
        macros: Vec::new(),
    };

    assert_eq!(encode(reply), frame(SMFIC_OPTNEG, &be32(&[6, 0x01, 0x00])));
}

#[test]
fn rpl_003_appends_the_macro_list_for_a_stage() {
    let reply = Reply::OptNeg {
        version: 6,
        actions: 0x01,
        protocol: 0x00,
        macros: vec![(SMFIM_ENVFROM, "{auth_authen} {mail_addr}".to_owned())],
    };

    let expected = [
        &be32(&[6, 0x01, 0x00, SMFIM_ENVFROM])[..],
        b"{auth_authen} {mail_addr}\0",
    ]
    .concat();
    assert_eq!(encode(reply), frame(SMFIC_OPTNEG, &expected));
}

#[test]
fn rpl_004_encodes_add_header() {
    let reply = Reply::AddHeader {
        name: "X-Test".to_owned(),
        value: "yes".to_owned(),
    };

    assert_eq!(encode(reply), frame(SMFIR_ADDHEADER, b"X-Test\0yes\0"));
}

#[test]
fn rpl_005_encodes_insert_header_with_its_index() {
    let reply = Reply::InsertHeader {
        index: 0,
        name: "X-Test".to_owned(),
        value: "yes".to_owned(),
    };

    let expected = [&be32(&[0])[..], b"X-Test\0yes\0"].concat();
    assert_eq!(encode(reply), frame(SMFIR_INSHEADER, &expected));
}

#[test]
fn rpl_006_encodes_change_header_with_an_empty_value() {
    let reply = Reply::ChangeHeader {
        index: 1,
        name: "X-Test".to_owned(),
        value: String::new(),
    };

    let expected = [&be32(&[1])[..], b"X-Test\0\0"].concat();
    assert_eq!(encode(reply), frame(SMFIR_CHGHEADER, &expected));
}

#[test]
fn rpl_007_encodes_esmtp_arguments_only_when_given() {
    let without_args = Reply::ChangeFrom {
        from: "<new@void.invalid>".to_owned(),
        args: None,
    };
    let with_args = Reply::ChangeFrom {
        from: "<new@void.invalid>".to_owned(),
        args: Some("SIZE=100".to_owned()),
    };
    let rcpt_with_args = Reply::AddRecipientWithArgs {
        rcpt: "<added@void.invalid>".to_owned(),
        args: Some("NOTIFY=NEVER".to_owned()),
    };

    assert_eq!(
        encode(without_args),
        frame(SMFIR_CHGFROM, b"<new@void.invalid>\0")
    );
    assert_eq!(
        encode(with_args),
        frame(SMFIR_CHGFROM, b"<new@void.invalid>\0SIZE=100\0")
    );
    assert_eq!(
        encode(rcpt_with_args),
        frame(SMFIR_ADDRCPT_PAR, b"<added@void.invalid>\0NOTIFY=NEVER\0")
    );
}

#[test]
fn rpl_008_rejects_a_body_chunk_over_65535_bytes() {
    assert_invalid(Reply::ReplaceBody(Bytes::from(vec![b'x'; 65_536])));
    encode(Reply::ReplaceBody(Bytes::from(vec![b'x'; 65_535])));
}

#[test]
fn rpl_009_rejects_invalid_header_names() {
    for name in ["X Test", "X:Test", "X-\u{7}Test"] {
        assert_invalid(Reply::AddHeader {
            name: name.to_owned(),
            value: "yes".to_owned(),
        });
    }
}

#[test]
fn rpl_010_rejects_a_reply_code_with_a_single_percent() {
    assert_invalid(Reply::ReplyCode("550 5.7.1 100% spam".to_owned()));
    encode(Reply::ReplyCode("550 5.7.1 100%% spam".to_owned()));
}

#[test]
fn rpl_011_rejects_an_empty_header_name() {
    assert_invalid(Reply::AddHeader {
        name: String::new(),
        value: "yes".to_owned(),
    });
}

#[test]
fn rpl_012_rejects_a_nul_byte_in_a_text_field() {
    assert_invalid(Reply::AddHeader {
        name: "X-Test".to_owned(),
        value: "y\0es".to_owned(),
    });
    assert_invalid(Reply::AddRecipient("<added\0@void.invalid>".to_owned()));
}

#[test]
fn rpl_013_rejects_reply_text_that_breaks_the_line_rules() {
    for text in [
        "250 OK",
        "550",
        "550-5.7.1 a",
        "550 5.7.1 a\r\n550 5.7.1 b",
        "550-a\r\n451 b",
        "550 4.7.1 x",
    ] {
        assert_invalid(Reply::ReplyCode(text.to_owned()));
    }
    encode(Reply::ReplyCode("550-5.7.1 a\r\n550 5.7.1 b".to_owned()));
}

// ReplyCode validation

/// The wire text a reply code turns into.
fn wire(code: ReplyCode) -> String {
    match Reply::from(code) {
        Reply::ReplyCode(text) => text,
        other => panic!("not a ReplyCode reply: {other:?}"),
    }
}

#[test]
fn rcd_001_builds_the_wire_text() {
    let code = ReplyCode::new(550, Some("5.7.1"), "Rejected by policy").unwrap();

    assert_eq!(wire(code), "550 5.7.1 Rejected by policy");
}

#[test]
fn rcd_002_rejects_codes_that_are_not_4xx_or_5xx() {
    for code in [250, 399, 600] {
        assert_eq!(
            ReplyCode::new(code, None, "text"),
            Err(ReplyError::InvalidCode(code))
        );
    }
}

#[test]
fn rcd_003_rejects_an_enhanced_code_with_a_different_class() {
    assert_eq!(
        ReplyCode::new(550, Some("4.7.1"), "text"),
        Err(ReplyError::EnhancedCodeMismatch)
    );
}

#[test]
fn rcd_004_rejects_malformed_enhanced_codes() {
    for enhanced in ["5.7", "5.7.1.1", "5.1000.1"] {
        assert_eq!(
            ReplyCode::new(550, Some(enhanced), "text"),
            Err(ReplyError::InvalidEnhancedCode),
            "{enhanced}"
        );
    }
}

#[test]
fn rcd_005_rejects_cr_and_lf_in_the_text() {
    for text in ["line\rbreak", "line\nbreak"] {
        assert_eq!(
            ReplyCode::new(550, None, text),
            Err(ReplyError::LineBreak),
            "{text:?}"
        );
    }
}

#[test]
fn rcd_006_accepts_980_bytes_of_text_and_rejects_981() {
    assert!(ReplyCode::new(550, None, &"x".repeat(980)).is_ok());
    assert_eq!(
        ReplyCode::new(550, None, &"x".repeat(981)),
        Err(ReplyError::LineTooLong { length: 981 })
    );
}

#[test]
fn rcd_007_doubles_each_percent_on_the_wire() {
    let code = ReplyCode::new(550, Some("5.7.1"), "100% spam").unwrap();

    assert_eq!(wire(code), "550 5.7.1 100%% spam");
}

#[test]
fn rcd_008_builds_a_multi_line_reply() {
    let code = ReplyCode::multi_line(550, Some("5.7.1"), &["a", "b", "c"]).unwrap();

    assert_eq!(wire(code), "550-5.7.1 a\r\n550-5.7.1 b\r\n550 5.7.1 c");
}

#[test]
fn rcd_009_accepts_32_lines_and_rejects_33() {
    let lines = vec!["line"; 33];

    assert!(ReplyCode::multi_line(550, None, &lines[..32]).is_ok());
    assert_eq!(
        ReplyCode::multi_line(550, None, &lines),
        Err(ReplyError::LineCount(33))
    );
}

#[test]
fn rcd_011_rejects_a_nul_byte_in_the_text() {
    assert_eq!(
        ReplyCode::new(550, None, "nul\0byte"),
        Err(ReplyError::NulByte)
    );
}

#[test]
fn rcd_012_rejects_a_multi_line_reply_without_lines() {
    let no_lines: [&str; 0] = [];

    assert_eq!(
        ReplyCode::multi_line(550, None, &no_lines),
        Err(ReplyError::LineCount(0))
    );
}

/// Postfix's per-line rules for a multi-line reply, from
/// `docs/reference/milter-protocol.md`, written out here so the test does not
/// reuse the library's own check.
fn passes_postfix_line_rules(wire: &str) -> bool {
    let mut lines: Vec<&[u8]> = Vec::new();
    for line in wire.split("\r\n") {
        lines.push(line.as_bytes());
    }
    for (index, line) in lines.iter().enumerate() {
        let separator = if index + 1 == lines.len() { b' ' } else { b'-' };
        // milter8_valid_smtp_reply: a digit after the separator must be the
        // code's first digit.
        let enhanced_matches = match line.get(4) {
            Some(next) if next.is_ascii_digit() => *next == line[0],
            _ => true,
        };
        let valid = line.len() >= 4
            && matches!(line[0], b'4' | b'5')
            && line[1].is_ascii_digit()
            && line[2].is_ascii_digit()
            && line[3] == separator
            && enhanced_matches;
        if !valid {
            return false;
        }
    }
    true
}

/// Sendmail's check: every `%` starts a `%%` pair.
fn passes_sendmail_percent_check(wire: &str) -> bool {
    let mut bytes = wire.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' && bytes.next() != Some(b'%') {
            return false;
        }
    }
    true
}

proptest! {
    #[test]
    fn rcd_010_every_valid_reply_code_passes_both_mtas_rules(
        code in 380u16..620,
        enhanced in proptest::option::of((0u16..10, 0u16..1_200, 0u16..1_200)),
        lines in proptest::collection::vec("[ a-zA-Z0-9%.:-]{0,40}", 0..35),
    ) {
        let enhanced = match enhanced {
            Some((class, subject, detail)) => Some(format!("{class}.{subject}.{detail}")),
            None => None,
        };
        let Ok(reply_code) = ReplyCode::multi_line(code, enhanced.as_deref(), &lines) else {
            return Ok(());
        };
        let reply = Reply::from(reply_code);

        let Reply::ReplyCode(wire) = &reply else {
            panic!("not a ReplyCode reply: {reply:?}");
        };
        prop_assert!(passes_postfix_line_rules(wire), "{wire:?}");
        prop_assert!(passes_sendmail_percent_check(wire), "{wire:?}");
        prop_assert!(!wire.contains('\0'));
        prop_assert!(reply.to_frame().is_ok());
    }
}

#[test]
fn rcd_013_rejects_text_that_starts_with_another_digit_without_an_enhanced_code() {
    assert_eq!(
        ReplyCode::new(550, None, "100 messages"),
        Err(ReplyError::EnhancedCodeMismatch)
    );
    assert!(ReplyCode::new(550, None, "5 messages").is_ok());
}

#[test]
fn cmd_044_rejects_bytes_after_the_last_field() {
    let cases: [(u8, &[u8]); 4] = [
        (SMFIC_HELO, b"client.void.invalid\0junk"),
        (SMFIC_UNKNOWN, b"XFOO bar\0junk"),
        (SMFIC_HEADER, b"Subject\0hi\0junk"),
        (
            SMFIC_CONNECT,
            b"mail.void.invalid\x004\x00\x19192.0.2.1\0junk",
        ),
    ];
    for (code, data) in cases {
        assert_malformed(Command::from_frame(frame(code, data)), code);
    }

    let longer = [&be32(&[6, 0x1FF, 0x1F_FFFF])[..], &[0, 0, 0, 0]].concat();
    assert_eq!(
        Command::from_frame(frame(SMFIC_OPTNEG, &longer)).unwrap(),
        Command::OptNeg {
            version: 6,
            actions: 0x1FF,
            protocol: 0x1F_FFFF
        }
    );
}

#[test]
fn rpl_014_rejects_header_values_that_would_add_header_lines() {
    for value in ["ok\nBcc: x@void.invalid", "a\rb", "a\n ", "a\n\t\nb"] {
        assert_invalid(Reply::AddHeader {
            name: "X-Test".to_owned(),
            value: value.to_owned(),
        });
    }
    encode(Reply::AddHeader {
        name: "X-Test".to_owned(),
        value: "first\n\tsecond".to_owned(),
    });
}

#[test]
fn rpl_015_rejects_empty_addresses_and_reasons() {
    for reply in [
        Reply::Quarantine(String::new()),
        Reply::AddRecipient(String::new()),
        Reply::DeleteRecipient(String::new()),
        Reply::ChangeFrom {
            from: String::new(),
            args: None,
        },
    ] {
        assert_invalid(reply);
    }
}

#[test]
fn rpl_016_rejects_a_new_header_name_that_is_not_printable_ascii() {
    assert_invalid(Reply::AddHeader {
        name: "X-Prüfung".to_owned(),
        value: "1".to_owned(),
    });
    assert_invalid(Reply::InsertHeader {
        index: 0,
        name: "X-Prüfung".to_owned(),
        value: "1".to_owned(),
    });
    encode(Reply::ChangeHeader {
        index: 1,
        name: "X-Prüfung".to_owned(),
        value: String::new(),
    });
}

#[test]
fn rpl_017_rejects_line_breaks_in_envelope_fields() {
    for reply in [
        Reply::AddRecipient("<a@void.invalid>\r\nRCPT TO:<b@void.invalid>".to_owned()),
        Reply::ChangeFrom {
            from: "<a@void.invalid>".to_owned(),
            args: Some("SIZE=1\nX".to_owned()),
        },
        Reply::Quarantine("held\rfor review".to_owned()),
    ] {
        assert_invalid(reply);
    }
}

#[test]
fn rcd_014_counts_the_line_limit_after_doubling_percent() {
    assert!(ReplyCode::new(550, None, &"%".repeat(490)).is_ok());
    assert_eq!(
        ReplyCode::new(550, None, &"%".repeat(491)),
        Err(ReplyError::LineTooLong { length: 982 })
    );
}
