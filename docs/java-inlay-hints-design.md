# Java Inlay Hints Reuse Audit

## Immediate reuse points

- `src/language/java/completion_context.rs`
  - Already materializes `var` locals by resolving initializer expressions into `LocalVar::type_internal`.
  - Already resolves method-argument semantics for expected-type inference, including receiver typing, overload selection, parameter mapping, and conservative fallback behavior.
- `src/language/java/expression_typing.rs`
  - Already owns expression typing for literals, chains, method calls, constructor calls, arrays, casts, and fallback recovery.
- `src/semantic/types.rs`
  - Already owns overload selection and parameter/return substitution logic.
- `src/language/java/render.rs`
  - Already owns user-facing Java type rendering.
- `src/language/java.rs`
  - Already builds `SemanticContext` from the source tree with locals, imports, current-class members, and `SourceTypeCtx`.

## Extraction seams

- Local type materialization in `completion_context` was completion-only and should be shared.
- Resolved-call computation for method-argument expected types was completion-only and should be shared.
- Inlay hints should only extract syntax sites, then consume the shared semantic queries above.

## Refactor direction

- Introduce a shared Java editor-semantics layer that exposes:
  - local type materialization
  - resolved invocation queries
  - parameter metadata lookup from resolved symbols
- Keep completion as one client of that layer.
- Keep inlay hints as another client of that layer.
- Keep LSP handlers transport-only.
