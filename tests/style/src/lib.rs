//! Checks that the Rust code of the repository keeps the procedural style
//! of ADR 0008: no closures and no `Fn`, `FnMut`, or `FnOnce` bounds, apart
//! from the exceptions the ADR names. It also reads the disallowed method
//! calls of `clippy.toml` from Clippy's output. The rules are in "Style
//! check" in `docs/reference/test-environment.md`.

use std::fmt;
use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, Token};

/// The methods whose closure argument is allowed: the library's builder
/// calls and `Once::call_once`.
const CLOSURE_METHODS: [&str; 3] = ["handler", "reload_with", "call_once"];

/// The builder items, by file and name, whose `Fn` bounds are allowed.
const BUILDER_ITEMS: [(&str, &str); 4] = [
    ("src/server/mod.rs", "Factory"),
    ("src/server/mod.rs", "handler"),
    ("src/server/mod.rs", "reload_with"),
    ("src/server/reload.rs", "Loader"),
];

/// The traits whose bounds are reported.
const FN_TRAITS: [&str; 6] = [
    "Fn",
    "FnMut",
    "FnOnce",
    "AsyncFn",
    "AsyncFnMut",
    "AsyncFnOnce",
];

/// The directories, from the repository root, whose Rust files are checked.
const DIRECTORIES: [&str; 3] = ["src", "tests", "examples"];

/// What a finding is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A closure outside an allowed call.
    Closure,
    /// A bound on the named `Fn` trait outside a builder item.
    FnBound(String),
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Closure => write!(f, "closure; use a named function or a loop (ADR 0008)"),
            Kind::FnBound(name) => write!(
                f,
                "{name} bound; take a value or a trait of your own instead (ADR 0008)"
            ),
        }
    }
}

/// One place that breaks the style. Findings sort in source order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
    /// What was found.
    pub kind: Kind,
}

/// The result of checking one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scan {
    /// The findings, in source order.
    pub findings: Vec<Finding>,
    /// Whether an attribute anywhere in the file allows or expects
    /// `clippy::disallowed_methods`.
    pub allows_disallowed_methods: bool,
}

/// Checks the source of the file `path`, given from the repository root.
pub fn scan(path: &str, source: &str) -> syn::Result<Scan> {
    let file = syn::parse_file(source)?;

    let mut checker = Checker {
        path,
        item: None,
        findings: Vec::new(),
        allows_disallowed_methods: false,
    };
    checker.visit_file(&file);
    let mut findings = checker.findings;
    findings.sort();

    Ok(Scan {
        findings,
        allows_disallowed_methods: checker.allows_disallowed_methods,
    })
}

/// `path` with its `.` and `..` components resolved.
fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// A call of a disallowed method, as Clippy reports it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MethodCall {
    /// The file, from the repository root.
    pub file: String,
    /// The line, from 1.
    pub line: usize,
    /// The column, from 1.
    pub column: usize,
    /// The method's path, such as `std::option::Option::map`.
    pub method: String,
}

/// The disallowed method calls in the output of `cargo clippy
/// --message-format=short`, in source order, each once. Clippy reports a
/// call once for each target that compiles the file; other lines are
/// skipped.
pub fn disallowed_method_calls(output: &str) -> Vec<MethodCall> {
    let mut calls = Vec::new();
    for line in output.lines() {
        if let Some(call) = parse_method_call(line)
            && !calls.contains(&call)
        {
            calls.push(call);
        }
    }
    calls.sort();
    calls
}

/// The call in one line of Clippy's short output, such as
/// ``src/a.rs:12:5: warning: use of a disallowed method `bool::then` ``.
fn parse_method_call(line: &str) -> Option<MethodCall> {
    let mut parts = line.splitn(4, ':');
    let file = parts.next()?;
    let line_number = parts.next()?.parse().ok()?;
    let column = parts.next()?.parse().ok()?;
    let message = parts.next()?.trim_start();
    let message = match message.strip_prefix("warning: ") {
        Some(rest) => rest,
        None => message.strip_prefix("error: ")?,
    };
    let method = message.strip_prefix("use of a disallowed method `")?;
    let method = method.strip_suffix('`')?;
    Some(MethodCall {
        file: normalize(file),
        line: line_number,
        column,
        method: method.to_owned(),
    })
}

/// The problems in Clippy's `output` for the repository at `root`, one
/// line each: a disallowed method call in a file that is not in `exempt`,
/// and a file in `exempt` that has nothing left to convert, with neither a
/// call in `output` nor a finding of [`scan`].
pub fn method_problems(
    root: &Path,
    output: &str,
    exempt: &[String],
) -> std::io::Result<Vec<String>> {
    let calls = disallowed_method_calls(output);
    let mut problems = Vec::new();
    for call in &calls {
        if !exempt.contains(&call.file) {
            problems.push(format!(
                "{}:{}:{}: use of a disallowed method `{}` (ADR 0008)",
                call.file, call.line, call.column, call.method
            ));
        }
    }

    for file in exempt {
        let mut has_call = false;
        for call in &calls {
            if &call.file == file {
                has_call = true;
            }
        }
        if has_call {
            continue;
        }
        let source = std::fs::read_to_string(root.join(file))?;
        let scanned = match scan(file, &source) {
            Ok(scanned) => scanned,
            Err(error) => {
                problems.push(format!("{file}: {error}"));
                continue;
            }
        };
        if scanned.findings.is_empty() {
            problems.push(format!(
                "{file} has nothing left to convert; remove it from tests/style/exemptions.txt"
            ));
        }
    }
    Ok(problems)
}

/// The repository root.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The exemption list, `tests/style/exemptions.txt`.
pub fn exemptions_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("exemptions.txt")
}

/// Every Rust file under the checked directories of `root`, from `root`
/// with `/` separators, sorted. `target` directories are skipped.
pub fn rust_files(root: &Path) -> std::io::Result<Vec<String>> {
    let mut files = Vec::new();
    for directory in DIRECTORIES {
        if root.join(directory).is_dir() {
            collect(root, directory, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

/// Adds the Rust files under `root/directory` to `files`.
fn collect(root: &Path, directory: &str, files: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(root.join(directory))? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let relative = format!("{directory}/{name}");
        if entry.file_type()?.is_dir() {
            if name != "target" {
                collect(root, &relative, files)?;
            }
        } else if name.ends_with(".rs") {
            files.push(relative);
        }
    }
    Ok(())
}

/// The file paths of an exemption list: one per line, with `#` comments
/// and blank lines skipped.
pub fn parse_exemptions(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in text.lines() {
        let line = match line.split_once('#') {
            Some((before, _)) => before,
            None => line,
        };
        let line = line.trim();
        if !line.is_empty() {
            files.push(line.to_owned());
        }
    }
    files
}

/// Whether `attribute` allows or expects `clippy::disallowed_methods`,
/// alone or with other lints.
fn allows_disallowed_methods(attribute: &syn::Attribute) -> bool {
    if !attribute.path().is_ident("allow") && !attribute.path().is_ident("expect") {
        return false;
    }
    let Ok(list) = attribute.meta.require_list() else {
        return false;
    };
    let mut tokens = String::new();
    for c in list.tokens.to_string().chars() {
        if !c.is_whitespace() {
            tokens.push(c);
        }
    }
    for lint in tokens.split(',') {
        if lint == "clippy::disallowed_methods" {
            return true;
        }
    }
    false
}

/// The arguments of a macro that takes expressions separated by commas.
struct Arguments(Punctuated<Expr, Token![,]>);

impl Parse for Arguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let arguments = Punctuated::parse_terminated(input)?;
        Ok(Arguments(arguments))
    }
}

/// Walks one file and records its findings.
struct Checker<'a> {
    /// The file, from the repository root.
    path: &'a str,
    /// The name of the innermost named item being walked.
    item: Option<String>,
    findings: Vec<Finding>,
    /// Whether an attribute allows or expects `clippy::disallowed_methods`.
    allows_disallowed_methods: bool,
}

impl Checker<'_> {
    fn report(&mut self, span: Span, kind: Kind) {
        let start = span.start();
        self.findings.push(Finding {
            line: start.line,
            column: start.column + 1,
            kind,
        });
    }

    /// Walks the body of a closure that is allowed where it stands.
    fn visit_allowed_closure(&mut self, closure: &syn::ExprClosure) {
        visit::visit_expr_closure(self, closure);
    }

    /// Walks the arguments of an allowed call: a closure among them is not
    /// reported, but what is inside it is.
    fn visit_allowed_arguments(&mut self, arguments: &Punctuated<Expr, Token![,]>) {
        for argument in arguments {
            if let Expr::Closure(closure) = argument {
                self.visit_allowed_closure(closure);
            } else {
                self.visit_expr(argument);
            }
        }
    }

    /// Whether the item being walked is a builder item.
    fn in_builder_item(&self) -> bool {
        let Some(item) = &self.item else {
            return false;
        };
        for (file, name) in BUILDER_ITEMS {
            if file == self.path && name == item {
                return true;
            }
        }
        false
    }

    /// Records that the walk entered the item `name`, and returns the name
    /// of the item around it, which the caller restores on the way out.
    fn enter(&mut self, name: String) -> Option<String> {
        self.item.replace(name)
    }
}

/// Whether `function` is `thread::spawn` or `std::thread::spawn`.
fn is_thread_spawn(function: &Expr) -> bool {
    let Expr::Path(path) = function else {
        return false;
    };
    let mut names = Vec::new();
    for segment in &path.path.segments {
        names.push(segment.ident.to_string());
    }
    names == ["thread", "spawn"] || names == ["std", "thread", "spawn"]
}

impl<'ast> Visit<'ast> for Checker<'_> {
    fn visit_attribute(&mut self, attribute: &'ast syn::Attribute) {
        if allows_disallowed_methods(attribute) {
            self.allows_disallowed_methods = true;
        }
        visit::visit_attribute(self, attribute);
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        self.report(closure.span(), Kind::Closure);
        visit::visit_expr_closure(self, closure);
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        let mut allowed = false;
        for name in CLOSURE_METHODS {
            if method == name {
                allowed = true;
            }
        }
        if !allowed {
            visit::visit_expr_method_call(self, call);
            return;
        }
        self.visit_expr(&call.receiver);
        self.visit_allowed_arguments(&call.args);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if !is_thread_spawn(&call.func) {
            visit::visit_expr_call(self, call);
            return;
        }
        self.visit_expr(&call.func);
        self.visit_allowed_arguments(&call.args);
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        // Macros whose arguments are expressions, such as assert! or vec!,
        // are checked. Others, such as macro_rules! or select!, are not.
        if let Ok(arguments) = mac.parse_body::<Arguments>() {
            for argument in &arguments.0 {
                self.visit_expr(argument);
            }
        }
    }

    fn visit_trait_bound(&mut self, bound: &'ast syn::TraitBound) {
        if let Some(last) = bound.path.segments.last() {
            let name = last.ident.to_string();
            for fn_trait in FN_TRAITS {
                if name == fn_trait && !self.in_builder_item() {
                    self.report(bound.span(), Kind::FnBound(name.clone()));
                }
            }
        }
        visit::visit_trait_bound(self, bound);
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let outer = self.enter(item.sig.ident.to_string());
        visit::visit_item_fn(self, item);
        self.item = outer;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let outer = self.enter(item.sig.ident.to_string());
        visit::visit_impl_item_fn(self, item);
        self.item = outer;
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        let outer = self.enter(item.sig.ident.to_string());
        visit::visit_trait_item_fn(self, item);
        self.item = outer;
    }

    fn visit_item_type(&mut self, item: &'ast syn::ItemType) {
        let outer = self.enter(item.ident.to_string());
        visit::visit_item_type(self, item);
        self.item = outer;
    }

    fn visit_item_struct(&mut self, item: &'ast syn::ItemStruct) {
        let outer = self.enter(item.ident.to_string());
        visit::visit_item_struct(self, item);
        self.item = outer;
    }

    fn visit_item_enum(&mut self, item: &'ast syn::ItemEnum) {
        let outer = self.enter(item.ident.to_string());
        visit::visit_item_enum(self, item);
        self.item = outer;
    }
}
