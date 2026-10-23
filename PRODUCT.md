# Product

## Users and purpose

Garage cluster administrators, read-only operators, and tenant users manage storage, access, and cluster operations in a self-hosted browser console. The Rust server embeds the React interface. See [Scope](docs/scope.md).

## Product character

Practical, clear, restrained. Preserve the existing Garage identity, system typography, Radix controls, and light/dark themes.

## Design principles

- Make navigation groups and the current location distinct.
- Use Radix color scales, 0.5 px borders, and at most four text sizes starting at 12 px.
- Keep copy concise; retain standard technical terms and units such as GB and TB. Do not use middle dots as text separators.
- Align controls and labels, use restrained corner radii, and minimize motion and decorative shadows.
- Show the target, consequence, pending state, and result of an operation.
- Keep routine actions easy to reach and retain graded confirmation for destructive actions.
- Explain errors with a useful next action; preserve input on failure.
- Support the existing deployment environments, including private-network HTTP access.

## Anti-references

Avoid visually indistinguishable navigation headings and links, nested maintenance controls without clear scope, and copy controls without actionable feedback.

## Accessibility

Preserve keyboard operation, visible focus, labeled controls, sufficient contrast, reduced-motion preferences, and layouts usable at 360 pixels. These requirements and interaction states are defined in [UI and interaction](docs/features-ui.md).
