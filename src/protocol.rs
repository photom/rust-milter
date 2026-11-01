//! Milter protocol constants, spelled as libmilter's `mfdef.h` spells them.
//!
//! The values are listed in `docs/reference/milter-protocol.md`.

/// Abort the current message.
pub const SMFIC_ABORT: u8 = b'A';
/// A body chunk.
pub const SMFIC_BODY: u8 = b'B';
/// Connection information.
pub const SMFIC_CONNECT: u8 = b'C';
/// Macro definitions.
pub const SMFIC_MACRO: u8 = b'D';
/// End of body.
pub const SMFIC_BODYEOB: u8 = b'E';
/// The HELO or EHLO command.
pub const SMFIC_HELO: u8 = b'H';
/// Quit; a new connection follows on the same socket.
pub const SMFIC_QUIT_NC: u8 = b'K';
/// A header field.
pub const SMFIC_HEADER: u8 = b'L';
/// The MAIL FROM command.
pub const SMFIC_MAIL: u8 = b'M';
/// End of headers.
pub const SMFIC_EOH: u8 = b'N';
/// Option negotiation.
pub const SMFIC_OPTNEG: u8 = b'O';
/// The QUIT command.
pub const SMFIC_QUIT: u8 = b'Q';
/// The RCPT TO command.
pub const SMFIC_RCPT: u8 = b'R';
/// The DATA command.
pub const SMFIC_DATA: u8 = b'T';
/// An SMTP command the MTA does not know.
pub const SMFIC_UNKNOWN: u8 = b'U';

/// Add a recipient.
pub const SMFIR_ADDRCPT: u8 = b'+';
/// Delete a recipient.
pub const SMFIR_DELRCPT: u8 = b'-';
/// Add a recipient with ESMTP arguments.
pub const SMFIR_ADDRCPT_PAR: u8 = b'2';
/// 421 shutdown; accepted from a milter by Postfix only.
pub const SMFIR_SHUTDOWN: u8 = b'4';
/// Accept the message.
pub const SMFIR_ACCEPT: u8 = b'a';
/// Replace the body (one chunk).
pub const SMFIR_REPLBODY: u8 = b'b';
/// Continue processing.
pub const SMFIR_CONTINUE: u8 = b'c';
/// Discard the message silently.
pub const SMFIR_DISCARD: u8 = b'd';
/// Change the envelope sender.
pub const SMFIR_CHGFROM: u8 = b'e';
/// Cause a connection failure; neither MTA accepts it.
pub const SMFIR_CONN_FAIL: u8 = b'f';
/// Add a header.
pub const SMFIR_ADDHEADER: u8 = b'h';
/// Insert a header at a position.
pub const SMFIR_INSHEADER: u8 = b'i';
/// Defined but never sent as a packet.
pub const SMFIR_SETSYMLIST: u8 = b'l';
/// Change or delete a header.
pub const SMFIR_CHGHEADER: u8 = b'm';
/// Progress: reset the MTA's read timeout.
pub const SMFIR_PROGRESS: u8 = b'p';
/// Quarantine the message.
pub const SMFIR_QUARANTINE: u8 = b'q';
/// Reject with a 5xx reply.
pub const SMFIR_REJECT: u8 = b'r';
/// Skip the remaining body chunks.
pub const SMFIR_SKIP: u8 = b's';
/// Temporary failure, a 4xx reply.
pub const SMFIR_TEMPFAIL: u8 = b't';
/// A custom SMTP reply code.
pub const SMFIR_REPLYCODE: u8 = b'y';

/// Unknown address family.
pub const SMFIA_UNKNOWN: u8 = b'U';
/// Unix or local socket.
pub const SMFIA_UNIX: u8 = b'L';
/// IPv4 address.
pub const SMFIA_INET: u8 = b'4';
/// IPv6 address.
pub const SMFIA_INET6: u8 = b'6';

/// No actions.
pub const SMFIF_NONE: u32 = 0x0000_0000;
/// Add headers.
pub const SMFIF_ADDHDRS: u32 = 0x0000_0001;
/// Change or replace the body.
pub const SMFIF_CHGBODY: u32 = 0x0000_0002;
/// Alias for [`SMFIF_CHGBODY`].
pub const SMFIF_MODBODY: u32 = SMFIF_CHGBODY;
/// Add recipients.
pub const SMFIF_ADDRCPT: u32 = 0x0000_0004;
/// Delete recipients.
pub const SMFIF_DELRCPT: u32 = 0x0000_0008;
/// Change or delete headers.
pub const SMFIF_CHGHDRS: u32 = 0x0000_0010;
/// Quarantine messages.
pub const SMFIF_QUARANTINE: u32 = 0x0000_0020;
/// Change the envelope sender.
pub const SMFIF_CHGFROM: u32 = 0x0000_0040;
/// Add recipients with ESMTP arguments.
pub const SMFIF_ADDRCPT_PAR: u32 = 0x0000_0080;
/// Set the macro list.
pub const SMFIF_SETSYMLIST: u32 = 0x0000_0100;

/// Version 1 filter actions.
pub const SMFI_V1_ACTS: u32 = 0x0000_000F;
/// Version 2 filter actions.
pub const SMFI_V2_ACTS: u32 = 0x0000_003F;
/// The current version's actions.
pub const SMFI_CURR_ACTS: u32 = 0x0000_01FF;

/// The MTA should not send connect information.
pub const SMFIP_NOCONNECT: u32 = 0x0000_0001;
/// The MTA should not send HELO information.
pub const SMFIP_NOHELO: u32 = 0x0000_0002;
/// The MTA should not send MAIL information.
pub const SMFIP_NOMAIL: u32 = 0x0000_0004;
/// The MTA should not send RCPT information.
pub const SMFIP_NORCPT: u32 = 0x0000_0008;
/// The MTA should not send the body.
pub const SMFIP_NOBODY: u32 = 0x0000_0010;
/// The MTA should not send headers.
pub const SMFIP_NOHDRS: u32 = 0x0000_0020;
/// The MTA should not send end of headers.
pub const SMFIP_NOEOH: u32 = 0x0000_0040;
/// No reply for headers.
pub const SMFIP_NR_HDR: u32 = 0x0000_0080;
/// Alias for [`SMFIP_NR_HDR`].
pub const SMFIP_NOHREPL: u32 = SMFIP_NR_HDR;
/// The MTA should not send unknown commands.
pub const SMFIP_NOUNKNOWN: u32 = 0x0000_0100;
/// The MTA should not send DATA.
pub const SMFIP_NODATA: u32 = 0x0000_0200;
/// The MTA understands `SMFIR_SKIP`.
pub const SMFIP_SKIP: u32 = 0x0000_0400;
/// The MTA should also send rejected recipients.
pub const SMFIP_RCPT_REJ: u32 = 0x0000_0800;
/// No reply for connect.
pub const SMFIP_NR_CONN: u32 = 0x0000_1000;
/// No reply for HELO.
pub const SMFIP_NR_HELO: u32 = 0x0000_2000;
/// No reply for MAIL.
pub const SMFIP_NR_MAIL: u32 = 0x0000_4000;
/// No reply for RCPT.
pub const SMFIP_NR_RCPT: u32 = 0x0000_8000;
/// No reply for DATA.
pub const SMFIP_NR_DATA: u32 = 0x0001_0000;
/// No reply for unknown commands.
pub const SMFIP_NR_UNKN: u32 = 0x0002_0000;
/// No reply for end of headers.
pub const SMFIP_NR_EOH: u32 = 0x0004_0000;
/// No reply for body chunks.
pub const SMFIP_NR_BODY: u32 = 0x0008_0000;
/// Keep the leading space of header values.
pub const SMFIP_HDR_LEADSPC: u32 = 0x0010_0000;
/// Maximum data size of 256 KB (`_FFR_MDS_NEGOTIATE` builds only).
pub const SMFIP_MDS_256K: u32 = 0x1000_0000;
/// Maximum data size of 1 MB (`_FFR_MDS_NEGOTIATE` builds only).
pub const SMFIP_MDS_1M: u32 = 0x2000_0000;

/// Version 1 protocol flags.
pub const SMFI_V1_PROT: u32 = 0x0000_003F;
/// Version 2 protocol flags.
pub const SMFI_V2_PROT: u32 = 0x0000_007F;
/// The current version's protocol flags.
pub const SMFI_CURR_PROT: u32 = 0x001F_FFFF;
/// Internal flags between the MTA and libmilter.
pub const SMFI_INTERNAL: u32 = 0x7000_0000;

/// Macros for the connect stage.
pub const SMFIM_CONNECT: u32 = 0;
/// Macros for the HELO stage.
pub const SMFIM_HELO: u32 = 1;
/// Macros for the MAIL FROM stage.
pub const SMFIM_ENVFROM: u32 = 2;
/// Macros for the RCPT TO stage.
pub const SMFIM_ENVRCPT: u32 = 3;
/// Macros for the DATA stage.
pub const SMFIM_DATA: u32 = 4;
/// Macros for the end of message stage.
pub const SMFIM_EOM: u32 = 5;
/// Macros for the end of headers stage.
pub const SMFIM_EOH: u32 = 6;

/// The lowest protocol version the library accepts.
pub const SMFI_PROT_VERSION_MIN: u32 = 2;
/// The current protocol version.
pub const SMFI_PROT_VERSION: u32 = 6;
