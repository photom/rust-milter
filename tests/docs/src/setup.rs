//! Prepares a document for rustdoc. `build.rs` and the unit tests share
//! this file.

/// The line that starts a setup comment.
const START: &str = "<!-- doctest-setup";
/// The line between the setup that goes before a block and the setup that
/// goes after it.
const SPLIT: &str = "---";
/// The line that ends a setup comment.
const END: &str = "-->";

/// Returns `document` with each setup comment merged into the Rust block
/// after it, and every Rust block marked `no_run`. Fails, naming the line,
/// at a setup comment that is not followed by a Rust block, at a code
/// block without a language, and at a comment or block that is not
/// closed.
pub fn prepare(document: &str) -> Result<String, String> {
    let lines: Vec<&str> = document.lines().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.trim() == START {
            let (setup, after) = setup_comment(&lines, index)?;
            let mut next = after;
            while next < lines.len() && lines[next].trim().is_empty() {
                push(&mut out, lines[next]);
                next += 1;
            }
            let mut rust_fence = None;
            if let Some(line) = lines.get(next) {
                if let Some(fence) = Fence::open(line) {
                    if fence.is_rust() {
                        rust_fence = Some(fence);
                    }
                }
            }
            let Some(fence) = rust_fence else {
                return Err(format!(
                    "line {}: a setup comment must come right before a Rust block",
                    index + 1
                ));
            };
            index = block(&lines, next, &fence, &setup, &mut out)?;
        } else if let Some(fence) = Fence::open(line) {
            index = block(&lines, index, &fence, &Setup::default(), &mut out)?;
        } else {
            push(&mut out, line);
            index += 1;
        }
    }
    Ok(out)
}

/// The lines of a setup comment, without the comment's indentation.
#[derive(Default)]
struct Setup {
    before: Vec<String>,
    after: Vec<String>,
}

/// Reads the setup comment that starts at `start`. Returns it and the
/// index of the line after its end.
fn setup_comment(lines: &[&str], start: usize) -> Result<(Setup, usize), String> {
    let indent = indentation(lines[start]);
    let mut setup = Setup::default();
    let mut split = false;
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        match line.trim() {
            END => return Ok((setup, index + 1)),
            SPLIT if !split => split = true,
            _ => {
                let line = line.strip_prefix(indent).unwrap_or(line).to_owned();
                if split {
                    setup.after.push(line);
                } else {
                    setup.before.push(line);
                }
            }
        }
    }
    Err(format!(
        "line {}: the setup comment is not closed with {END}",
        start + 1
    ))
}

/// The opening line of a fenced code block.
struct Fence<'a> {
    indent: &'a str,
    /// The number of backticks.
    ticks: usize,
    /// The info string: the language and rustdoc's attributes.
    info: &'a str,
}

impl<'a> Fence<'a> {
    fn open(line: &'a str) -> Option<Fence<'a>> {
        let indent = indentation(line);
        let rest = &line[indent.len()..];
        let ticks = rest.len() - rest.trim_start_matches('`').len();
        if ticks < 3 {
            return None;
        }
        Some(Fence {
            indent,
            ticks,
            info: rest[ticks..].trim(),
        })
    }

    fn attributes(&self) -> Vec<&'a str> {
        let mut attributes = Vec::new();
        for attribute in self.info.split([',', ' ', '\t']) {
            if !attribute.is_empty() {
                attributes.push(attribute);
            }
        }
        attributes
    }

    fn is_rust(&self) -> bool {
        self.attributes().first() == Some(&"rust")
    }

    /// Whether the block already has an attribute that keeps rustdoc from
    /// running it: `ignore`, `no_run`, or `compile_fail`.
    fn skips_run(&self) -> bool {
        for attribute in self.attributes() {
            if matches!(attribute, "ignore" | "no_run" | "compile_fail") {
                return true;
            }
        }
        false
    }

    /// Whether `line` closes this block.
    fn closed_by(&self, line: &str) -> bool {
        let line = line.trim();
        line.len() >= self.ticks && line.trim_start_matches('`').is_empty()
    }
}

/// Copies the code block that opens at `start` into `out`, with `setup`
/// around its code. Returns the index of the line after the block.
fn block(
    lines: &[&str],
    start: usize,
    fence: &Fence<'_>,
    setup: &Setup,
    out: &mut String,
) -> Result<usize, String> {
    if fence.info.is_empty() {
        return Err(format!(
            "line {}: a code block needs a language, because rustdoc compiles a block \
             without one as Rust; mark text that is not code as `text`",
            start + 1
        ));
    }
    if fence.is_rust() && !fence.skips_run() {
        push(out, &format!("{},no_run", lines[start].trim_end()));
    } else {
        push(out, lines[start]);
    }
    for line in &setup.before {
        push(out, &format!("{}{line}", fence.indent));
    }
    for (index, line) in lines.iter().enumerate().skip(start + 1) {
        if fence.closed_by(line) {
            for line in &setup.after {
                push(out, &format!("{}{line}", fence.indent));
            }
            push(out, line);
            return Ok(index + 1);
        }
        push(out, line);
    }
    Err(format!("line {}: the code block is not closed", start + 1))
}

/// The leading spaces and tabs of `line`.
fn indentation(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

fn push(out: &mut String, line: &str) {
    out.push_str(line);
    out.push('\n');
}
