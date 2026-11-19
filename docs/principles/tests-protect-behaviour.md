# A test protects a behaviour

Keep a test only if it would fail when something a user or caller relies on breaks. Don't test that text is on a page, that the framework works, or how the code is built inside. An assertion written to drive a removal ("X is gone") goes away with X.

Why: tests that pin details run slower, break on every refactor and hide the few that matter.
