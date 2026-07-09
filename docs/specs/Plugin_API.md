# Plugin API

## Purpose

Describe how third-party extensions can add value to Pipe Deck without compromising safety or clarity.

## In Scope

- Extension boundaries and capability model.
- Plugin lifecycle and versioning strategy.
- Security constraints for OSS ecosystem participation.

## Out of Scope

- Final transport/runtime protocol.
- Full SDK implementation details.

## Plugin Goals

- Enable ecosystem-driven enhancements.
- Keep core routing behavior reliable.
- Allow opt-in features without bloating baseline product.

## Extension Boundaries

Potential extension areas:

- Rule suggestion providers.
- Profile import/export translators.
- Device labeling and categorization helpers.
- External integration connectors.

Restricted areas (core-owned):

- Direct unrestricted PipeWire mutation.
- Core safety policy bypass.
- Privileged background operations without explicit approval.

## Capability Model (Draft)

Plugins declare required capabilities, such as:

- Read routing graph
- Suggest routing changes
- Read/write profile metadata
- Register UI panels

Capabilities should be explicit, reviewable, and revocable.

## Lifecycle (Draft)

- Discover plugin manifest.
- Validate compatibility and permissions.
- Initialize with scoped context.
- Run with event subscriptions and bounded resources.
- Shutdown cleanly and release handles.

## Versioning and Compatibility

- Semantic versioning for plugin API surface.
- Compatibility matrix in docs.
- Deprecation windows before breaking removals.

## Security Constraints

- Principle of least privilege.
- Explicit user approval for high-impact capabilities.
- Audit log entries for plugin-triggered changes.

## Decisions

- Plugins run in isolated subprocesses by default.
- Capability requests are explicit and denied by default until approved.
- Plugin failures must not crash or block core routing operations.

## Traceability to User Value

- Ecosystem plugins -> faster innovation for niche Linux audio setups.
- Capability model -> safer extension without hidden behavior.

## Versioning Notes

- Initial plugin support prioritizes safety and compatibility over broad surface area.
