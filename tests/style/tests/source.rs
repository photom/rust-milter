//! The style check on one source file at a time. The scenarios are in
//! `docs/testing/style.md`.

use rust_milter_style::{Kind, scan};

/// The line and kind of every finding in `source`, read as the file
/// `path`.
fn findings(path: &str, source: &str) -> Vec<(usize, Kind)> {
    let scan = scan(path, source).unwrap();
    let mut found = Vec::new();
    for finding in scan.findings {
        found.push((finding.line, finding.kind));
    }
    found
}

#[test]
fn sty_001_a_closure_is_reported_with_its_line() {
    let source = "fn f(v: Vec<u8>) -> Vec<u8> {\n\
                  \x20   let double = |x: u8| x * 2;\n\
                  \x20   v.into_iter().map(double).collect()\n\
                  }\n";

    assert_eq!(findings("src/a.rs", source), [(2, Kind::Closure)]);
}

#[test]
fn sty_002_a_closure_passed_to_an_allowed_call_is_not_reported() {
    let source = "fn f() {\n\
                  \x20   let builder = Server::builder().handler(|_| Handler);\n\
                  \x20   let gauge = meter.u64_observable_gauge(\"g\").with_callback(move |o| o.observe(1, &[]));\n\
                  \x20   ONCE.call_once(|| init());\n\
                  \x20   std::thread::spawn(|| work());\n\
                  \x20   thread::spawn(move || work());\n\
                  }\n";

    assert_eq!(findings("src/a.rs", source), []);
}

#[test]
fn sty_002_a_closure_inside_an_allowed_closure_is_reported() {
    let source = "fn f() {\n\
                  \x20   thread::spawn(|| {\n\
                  \x20       let g = |x: u8| x;\n\
                  \x20   });\n\
                  }\n";

    assert_eq!(findings("src/a.rs", source), [(3, Kind::Closure)]);
}

#[test]
fn sty_002_a_closure_passed_to_another_call_is_reported() {
    let source = "fn f() {\n\
                  \x20   tokio::task::spawn_blocking(|| work());\n\
                  \x20   other::spawn(|| work());\n\
                  \x20   builder.reload_with(|| async move { load().await });\n\
                  }\n";

    assert_eq!(
        findings("src/a.rs", source),
        [(2, Kind::Closure), (3, Kind::Closure), (4, Kind::Closure)]
    );
}

#[test]
fn sty_003_an_async_block_is_not_reported_and_an_async_closure_is() {
    let source = "async fn f() {\n\
                  \x20   let task = async move { work().await };\n\
                  \x20   let closure = async |x: u8| x;\n\
                  }\n";

    assert_eq!(findings("src/a.rs", source), [(3, Kind::Closure)]);
}

#[test]
fn sty_004_a_closure_in_the_arguments_of_a_macro_is_reported() {
    let source = "fn f(v: &[u8]) {\n\
                  \x20   assert!(v.iter().any(|x| *x == 0), \"no zero\");\n\
                  \x20   println!(\"{}\", v.len());\n\
                  }\n";

    assert_eq!(findings("src/a.rs", source), [(2, Kind::Closure)]);
}

#[test]
fn sty_005_every_fn_trait_bound_is_reported() {
    let source = "fn a(f: impl Fn(u8) -> u8) {}\n\
                  fn b<F: FnMut()>(f: F) {}\n\
                  fn c<F>(f: F) where F: FnOnce() {}\n\
                  type D = Box<dyn Fn() + Send>;\n\
                  fn e() -> impl FnOnce(&mut u8) { todo!() }\n";

    assert_eq!(
        findings("tests/a.rs", source),
        [
            (1, Kind::FnBound("Fn".to_owned())),
            (2, Kind::FnBound("FnMut".to_owned())),
            (3, Kind::FnBound("FnOnce".to_owned())),
            (4, Kind::FnBound("Fn".to_owned())),
            (5, Kind::FnBound("FnOnce".to_owned())),
        ]
    );
}

#[test]
fn sty_006_only_the_builder_methods_may_have_a_fn_bound() {
    let source = "type Factory<H, A> = Box<dyn FnOnce(&HandlerContext<A>) -> H>;\n\
                  impl<A> ServerBuilder<(), A> {\n\
                  \x20   pub fn handler<H, F>(self, factory: F) -> ServerBuilder<H, A>\n\
                  \x20   where\n\
                  \x20       F: FnOnce(&HandlerContext<A>) -> H + 'static,\n\
                  \x20   { todo!() }\n\
                  \x20   pub fn other<F: Fn()>(self, f: F) {}\n\
                  \x20   pub fn build(self) -> Server where F: FnOnce(&HandlerContext<A>) -> H { todo!() }\n\
                  \x20   pub fn reload_with<F: Fn()>(self, loader: F) -> Self { todo!() }\n\
                  }\n";

    assert_eq!(
        findings("src/server/mod.rs", source),
        [
            (1, Kind::FnBound("FnOnce".to_owned())),
            (7, Kind::FnBound("Fn".to_owned())),
            (9, Kind::FnBound("Fn".to_owned()))
        ]
    );
}

#[test]
fn sty_006_a_builder_item_name_in_another_file_is_reported() {
    let source = "fn handler<F: FnOnce()>(factory: F) {}\n";

    assert_eq!(
        findings("tests/support/mod.rs", source),
        [(1, Kind::FnBound("FnOnce".to_owned()))]
    );
}

#[test]
fn sty_007_an_allow_or_expect_of_disallowed_methods_is_detected() {
    let with = "#![allow(clippy::disallowed_methods)]\nfn f() {}\n";
    let other = "#![allow(clippy::needless_return)]\nfn f() {}\n";
    let without = "fn f() {}\n";
    let on_item = "#[allow(dead_code, clippy::disallowed_methods)]\nfn f() {}\n";
    let expected = "fn f() {\n    #[expect(clippy::disallowed_methods)]\n    let x = 1;\n}\n";

    assert!(scan("src/a.rs", with).unwrap().allows_disallowed_methods);
    assert!(scan("src/a.rs", on_item).unwrap().allows_disallowed_methods);
    assert!(
        scan("src/a.rs", expected)
            .unwrap()
            .allows_disallowed_methods
    );
    assert!(!scan("src/a.rs", other).unwrap().allows_disallowed_methods);
    assert!(!scan("src/a.rs", without).unwrap().allows_disallowed_methods);
}

#[test]
fn sty_008_every_function_pointer_type_is_reported() {
    let source = "fn a(f: fn(u8) -> u8) {}\n\
                  struct B { check: fn(&str) -> bool }\n\
                  type C = fn();\n\
                  fn d() -> Vec<fn(u8)> { todo!() }\n\
                  fn e() { let table: [(u8, fn(u8)); 0] = []; }\n";

    assert_eq!(
        findings("src/a.rs", source),
        [
            (1, Kind::FnPointer),
            (2, Kind::FnPointer),
            (3, Kind::FnPointer),
            (4, Kind::FnPointer),
            (5, Kind::FnPointer),
        ]
    );
}

#[test]
fn sty_008_a_function_item_is_not_a_function_pointer() {
    let source = "fn a(x: u8) -> u8 { x }\n\
                  trait B { fn b(&self); }\n\
                  impl B for u8 { fn b(&self) {} }\n";

    assert_eq!(findings("src/a.rs", source), []);
}
