# Shared component extension rules

The shared components are ORION's UI foundation. Feature teams compose these
components; they do not copy, fork, or restyle them into a second design system.
This foundation depends on SHAURYA-01.

## Ownership

- Shaurya is the only direct editor of `shared/ui`, `shared/forms`,
  `shared/tables`, `shared/layouts`, and the shared rules in `global.css`.
- Feature owners may import and compose shared components in their feature
  modules.
- Changes to a shared component are proposed to Shaurya with a concrete use
  case. Do not work around a missing capability by copying its implementation.
- Feature-specific business logic, API calls, route decisions, and data
  transformation remain in the owning feature.

## Prefer composition

Use existing props, children, and render functions before requesting a new API.

```tsx
<Card
  header={<ReportHeading report={report} />}
  footer={<Button onClick={openReport}>Open report</Button>}
>
  <ReportSummary report={report} />
</Card>
```

Do not copy `Card`, add a feature-prefixed version, target its internal CSS
selectors, or reproduce its styles in a feature stylesheet.

A shared extension is appropriate only when all of these are true:

1. At least two feature contexts need the same behavior or the behavior is a
   foundational accessibility requirement.
2. The capability is domain-neutral.
3. Composition cannot express it without depending on component internals.
4. Its default, interactive/focused, loading/disabled, and error/success states
   are defined where applicable.

## API rules

- Preserve native HTML behavior and forward relevant native props.
- Use explicit, typed props. Do not add untyped configuration objects or
  feature-specific flags.
- Support controlled state when application state must own the value. A
  component may also support an uncontrolled default when that is useful.
- Name state props consistently: `isLoading`, `disabled`, `error`, `success`,
  `value`, `defaultValue`, and `onChange`/`onValueChange`.
- Keep defaults safe and useful. Optional visual content uses `ReactNode`;
  behavior uses callbacks.
- Additive changes are preferred. A breaking change requires a consumer audit,
  migration notes, and coordinated updates.
- Do not expose internal class names, DOM structure, refs, or state as a public
  customization API unless a demonstrated accessibility need requires it.

## Accessibility requirements

Behavioral accessibility is required; visual similarity is not sufficient.

- Start with the correct native element and semantic landmark.
- Every control has an accessible name. Help and error text is programmatically
  associated with its control.
- Keyboard interaction follows the applicable ARIA Authoring Practices pattern.
- Focus is visible, logical, and restored after temporary UI such as dialogs.
- Loading, errors, and success updates are announced without moving focus
  unnecessarily.
- Disabled behavior prevents activation and remains understandable.
- Do not add positive `tabIndex` values or remove focus outlines.
- Support reduced motion and forced-colors/high-contrast modes.
- Responsive behavior must preserve reading order, keyboard order, and access to
  every action.

## Styling and responsive rules

- Consume the `--ui-*` tokens in `global.css`; do not introduce feature-local
  colors, spacing scales, focus rings, radii, or breakpoints for shared UI.
- Use fluid layout tokens before adding a media query. Shared breakpoint tiers
  are `30rem`, `48rem`, `64rem`, and `80rem`.
- Component styles use the `ui-` prefix and remain in `global.css` until the
  repository adopts an approved shared styling module strategy.
- Feature styles may control surrounding layout but must not reach into
  `.ui-*` descendants or override component lifecycle states.
- New variants must communicate meaning, work in forced colors, and have a
  domain-neutral name such as `danger` or `success`.

## Required proposal

A request to extend the foundation should include:

- the user need and at least two intended contexts;
- why current composition is insufficient;
- the proposed typed API with defaults;
- lifecycle and responsive behavior;
- keyboard, focus, labeling, and announcement behavior;
- compatibility and migration impact.

Shaurya decides whether the outcome is a new component, an additive prop, a
composition example, or feature-owned code.

## Required validation

Every shared change includes an executable example when it introduces a new
pattern and behavioral tests for relevant states. Run:

```bash
npm run type-check
npm run lint
npm run test
npm run build
```

The accessibility automation must remain green. Tests should query components
by role, accessible name, label, and state rather than by internal class name.
Visual-only snapshots do not replace interaction assertions.

## Review checklist

- [ ] The capability is shared and domain-neutral.
- [ ] Existing composition cannot meet the need cleanly.
- [ ] The API is typed, native-friendly, and backward compatible.
- [ ] Applicable lifecycle states are implemented.
- [ ] Keyboard, focus, labeling, announcements, and responsive behavior work.
- [ ] Shared tokens are used without feature overrides.
- [ ] Examples and behavioral tests are included.
- [ ] Type-check, lint, tests, accessibility automation, and build pass.
