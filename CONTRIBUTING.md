# Contributing to HexaGraph

Thank you for helping!

* **Statistics must come with tests.** Each new method needs unit tests that
  compare it with reference values from R or SciPy, and the test should say
  which R/SciPy call produced the numbers.
* **Clean room.** Implement from textbooks and papers, and cite them in the doc
  comments. Do not copy code, text, icons or artwork from proprietary
  software.
* Run `cargo fmt`, `cargo clippy --workspace --all-targets` and
  `cargo test --workspace` before opening a pull request.
* The `hg-stats` crate has no GUI dependencies. Keep it that way so it stays
  usable as a library.
