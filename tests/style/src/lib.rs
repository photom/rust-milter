//! Checks that the Rust code of the repository keeps the procedural style
//! of ADRs 0008 to 0010: no closures, no `Fn`, `FnMut`, or `FnOnce`
//! bounds, and no function pointer types, apart from the exceptions the
//! ADRs name. It also reads the disallowed method calls of `clippy.toml`
//! from Clippy's output. The rules are in "Style check" in
//! `docs/reference/test-environment.md`.

use std::fmt;
use std::path::{Path, PathBuf};

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};
use syn::{Expr, Token};

/// The methods whose closure argument is allowed: the handler factory of
/// the builder, `Once::call_once`, and OpenTelemetry's `with_callback`.
const CLOSURE_METHODS: [&str; 3] = ["handler", "call_once", "with_callback"];

/// The builder methods, by file and name, whose `Fn` bounds are allowed:
/// `handler` takes the handler factory, and `build` calls it.
const BUILDER_METHODS: [(&str, &str); 2] = [
    ("src/server/mod.rs", "handler"),
    ("src/server/mod.rs", "build"),
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
    /// A bound on the named `Fn` trait outside a builder method.
    FnBound(String),
    /// A function pointer type, such as `fn(u8) -> bool`.
    FnPointer,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Closure => write!(f, "closure; use a named function or a loop (ADR 0008)"),
            Kind::FnBound(name) => write!(
                f,
                "{name} bound; take a value or a trait of your own instead (ADR 0008)"
            ),
            Kind::FnPointer => write!(
                f,
                "function pointer; name the cases with an enum and match on it (ADR 0009)"
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
    /// The first and last lines of each `derive` attribute, alone or inside
    /// `cfg_attr`. Clippy reports a call that a derive macro generates on
    /// these lines.
    pub derive_lines: Vec<(usize, usize)>,
}

impl Scan {
    /// Whether `line` belongs to a `derive` attribute.
    pub fn is_derive_line(&self, line: usize) -> bool {
        for (first, last) in &self.derive_lines {
            if *first <= line && line <= *last {
                return true;
            }
        }
        false
    }
}

/// Checks the source of the file `path`, given from the repository root.
pub fn scan(path: &str, source: &str) -> syn::Result<Scan> {
    let file = syn::parse_file(source)?;

    let mut checker = Checker {
        path,
        item: None,
        findings: Vec::new(),
        allows_disallowed_methods: false,
        derive_lines: Vec::new(),
    };
    checker.visit_file(&file);
    let mut findings = checker.findings;
    findings.sort();

    Ok(Scan {
        findings,
        allows_disallowed_methods: checker.allows_disallowed_methods,
        derive_lines: checker.derive_lines,
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
/// skipped. Terminal colors, as with `CARGO_TERM_COLOR=always`, are
/// ignored.
pub fn disallowed_method_calls(output: &str) -> Vec<MethodCall> {
    let mut calls = Vec::new();
    for line in output.lines() {
        if let Some(call) = parse_method_call(&without_colors(line))
            && !calls.contains(&call)
        {
            calls.push(call);
        }
    }
    calls.sort();
    calls
}

/// `line` without its ANSI escape sequences, such as `ESC [ 1 ; 33 m`. A
/// sequence ends at its first byte in the range `@` to `~`.
fn without_colors(line: &str) -> String {
    let mut text = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            text.push(c);
            continue;
        }
        if chars.next() != Some('[') {
            continue;
        }
        for code in chars.by_ref() {
            if ('@'..='~').contains(&code) {
                break;
            }
        }
    }
    text
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
/// line each: every disallowed method call, except a call that a derive
/// macro generates, which is not the repository's code.
pub fn method_problems(root: &Path, output: &str) -> std::io::Result<Vec<String>> {
    let mut scans: Vec<(String, Scan)> = Vec::new();
    let mut problems = Vec::new();
    for call in disallowed_method_calls(output) {
        if generated_by_derive(root, &call, &mut scans)? {
            continue;
        }
        problems.push(format!(
            "{}:{}:{}: use of a disallowed method `{}` (ADR 0008)",
            call.file, call.line, call.column, call.method
        ));
    }
    Ok(problems)
}

/// Whether a derive macro generated `call`: Clippy reports it on the lines
/// of a `derive` attribute. `scans` keeps the files read so far.
fn generated_by_derive(
    root: &Path,
    call: &MethodCall,
    scans: &mut Vec<(String, Scan)>,
) -> std::io::Result<bool> {
    for (file, scanned) in scans.iter() {
        if *file == call.file {
            return Ok(scanned.is_derive_line(call.line));
        }
    }
    let source = std::fs::read_to_string(root.join(&call.file))?;
    let Ok(scanned) = scan(&call.file, &source) else {
        return Ok(false);
    };
    let generated = scanned.is_derive_line(call.line);
    scans.push((call.file.clone(), scanned));
    Ok(generated)
}

/// The repository root.
pub fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
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

/// Whether `attribute` is `#[derive(...)]`, or a `cfg_attr` that holds one.
fn is_derive(attribute: &syn::Attribute) -> bool {
    if attribute.path().is_ident("derive") {
        return true;
    }
    if !attribute.path().is_ident("cfg_attr") {
        return false;
    }
    let Ok(list) = attribute.meta.require_list() else {
        return false;
    };
    for token in list.tokens.clone() {
        if let proc_macro2::TokenTree::Ident(ident) = token
            && ident == "derive"
        {
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
    /// The lines of each `derive` attribute.
    derive_lines: Vec<(usize, usize)>,
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

    /// Whether the item being walked is a builder method.
    fn in_builder_method(&self) -> bool {
        let Some(item) = &self.item else {
            return false;
        };
        for (file, name) in BUILDER_METHODS {
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
        if is_derive(attribute) {
            let span = attribute.span();
            self.derive_lines.push((span.start().line, span.end().line));
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
                if name == fn_trait && !self.in_builder_method() {
                    self.report(bound.span(), Kind::FnBound(name.clone()));
                }
            }
        }
        visit::visit_trait_bound(self, bound);
    }

    fn visit_type_fn_ptr(&mut self, pointer: &'ast syn::TypeFnPtr) {
        self.report(pointer.span(), Kind::FnPointer);
        visit::visit_type_fn_ptr(self, pointer);
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
