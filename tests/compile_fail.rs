//! Inputs the macros must reject, with the errors they give.
//!
//! Each `tests/ui/**/*.rs` file must fail to compile with exactly the error in
//! the `.stderr` file next to it. After changing an error on purpose,
//! regenerate them with `TRYBUILD=overwrite cargo test --test compile_fail`
//! and review the diff.
//!
//! Errors that come from rustc rather than from the macros (e.g. a missing
//! trait) may change between compiler versions; regenerate them then.

#[test]
fn compile_fail() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/make_aggregate/*.rs");
    cases.compile_fail("tests/ui/functions/*.rs");
    cases.compile_fail("tests/ui/derives/*.rs");
}
