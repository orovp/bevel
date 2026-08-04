---
name: angular-architecture
description: Architecture and conventions for an Angular SPA that is downstream of a generated backend contract, where one bundle serves every host and Angular Material is the only UI system. Use when adding an entity feature, a list view, a form, an API service or a shared component to such an app, when wiring a foreign-key control or a third-party editor, or when reviewing whether an Angular change respects the layering.
---

# Angular architecture

The shape of a front end **downstream of a typed contract it does not own**:
entity models are generated from the backend's schema, and one built bundle
serves the dev server, the embedded daemon and the desktop shell. Applies where
`@angular/material` is the only UI library, component state is signals, and
`{Entity}Api` is the only place `HttpClient` appears.

**Generic Angular is assumed, not repeated.** Standalone components, `inject()`,
`OnPush`, signals, built-in control flow, `track` on every `@for` — already the
defaults, and the compiler or the lint config rejects the alternatives. What
follows is only what this codebase decided, and what a good Angular developer
gets wrong anyway.

**Green lint is not the bar; the production build is.** `strictTemplates` and
AOT run only there, so lint and the unit suite stay green over a template
reading a field the model does not have. Nothing is done until `ng build` is.

**The order is a ranking by what being wrong costs**: the earliest rules fail
most silently and survive `ng build`; the last are what a sibling file
half-teaches. Read top-down under a budget; if two rules collide, the earlier
wins.

## 1. One bundle, three hosts: the base URL is injected, never written

```ts
private readonly baseUrl = `${inject(API_BASE_URL)}/{entity_plural}`;
```

- **Never a literal host, never a build-time `environment.ts`.** An environment
  file means one bundle per mode, which is what the runtime token exists to
  avoid — the desktop shell ships the very same `dist`.
- **Never a bare relative path in a service.** Inside the desktop shell the
  document origin is `tauri://localhost` (`http://tauri.localhost` on Windows),
  so `/api/v1/...` resolves against the WebView, not the daemon. It works under
  `ng serve`, in the tests and in the embedded server; it fails only in the
  packaged app, which is the last place anyone looks.
- The value resolves **before `bootstrapApplication`**, from a served
  `config.json`, spread into the providers. A missing or malformed file is
  **not an error** — it means same origin, which is what the dev proxy, the
  single-port server and the token's `providedIn: 'root'` default already give.

## 2. The models are generated; the contract is upstream

| Generated file | Produced by |
| --- | --- |
| `core/api-types.generated.ts` | `openapi-typescript` over the generated schema |
| `features/{entity}/{entity}.model.ts` | `cargo xtask gen-ts` — one shim per entity |
| `core/search/search.model.ts` | `cargo xtask gen-ts` — the result union and its arms |

**To change a field, change the backend DTO**, then regenerate. TypeScript stops
compiling at every call site that has to change — that is the mechanism, not an
inconvenience. Editing a generated file is worse than useless: the contract
check fails in CI and the next generation overwrites it.

- **Never invent a field, endpoint or query parameter absent from the
  contract** — it type-checks against nothing and fails at runtime.
- **The generator writes a model only into a feature folder that already
  exists.** Create `src/app/features/{entity}/` first, *then* regenerate.
  Hand-writing the model to get moving produces a file that looks right, passes
  review, and is silently replaced by a different shape later.
- **That folder keeps the contract's spelling, underscores included**
  (`features/task_audit/`, `task_audit-list.ts`), while the class is
  `TaskAuditList` and the selector `app-task-audit-list`. Renaming the folder to
  kebab-case is where the generator stops finding it.
- **One model stays hand-written: `core/pagination.model.ts`.** `Paginated<T>`
  is generic on the server and the schema generator monomorphises it per entity
  with the item inlined, so there is no generic left to generate; its item type
  still comes from a generated model. Any second exception says why at the top
  of the file, or the generator's boundary erodes.
- **The big generated file is excluded from lint on purpose** — reformatting it
  to please a rule desynchronises it from the contract check.
- **Narrow a union with `Extract<Item, { entity_type: 'X' }>`; never re-declare
  the arm.** The bug this caught: audit rows call the field `note`, everything
  else calls it `notes`; the hand-written model said `notes`, so that branch
  returned `undefined` for every audit result and rendered nothing, silently.

## 3. Forms: one component both ways, and `''` is not `null`

One form component serves create and edit; edit mode is the presence of `:id` in
`route.snapshot.paramMap`, read once in the constructor, which then loads the
entity and `patchValue`s it. On success: snackbar, then navigate back to the
list. On `form.invalid`: return, and let the inline `mat-error`s speak.

- **Optional string fields carry `minLength: 1` in the schema, so `''` is not
  "absent".** Map `''` → `null` on submit and `null` → `''` on patch, both
  directions, every optional string. Skip it and a form that renders as valid
  earns a 400 the interceptor reports as a mystery.
- `fb.nonNullable.group(...)` with validators mirroring the schema; foreign keys
  are `fb.control<number | null>(null)`, so submit reads `getRawValue()` and
  asserts the required ones.
- **The autocomplete's text is not the value.** A large option set gets a second
  `FormControl`, outside the form group, holding display text while the form
  control holds the id. Two rules make it correct: typing anything that no
  longer equals the selected label **clears the id**, and — because the entity
  and its option list arrive in either order — the loaded id parks in a
  `pending{X}Id` that **both** callbacks re-run the resolver from. Resolving in
  only one of them leaves the field blank on edit whenever that race goes the
  other way, which it never does on the developer's machine.
- **A small closed set stays a `<mat-select>`** filled from the referenced
  entity's `getAll(100)`: store the id, never raw text, and load the options
  unconditionally — create mode needs them too.

## 4. Material is the system, and the one exception is fenced

Never hand-roll what Material provides, add no second UI or state-management
library, and let RxJS stop where `HttpClient` hands over an Observable. The
rich-text editor is the one exception, confined to `shared/markdown-editor/`
plus one block in the global stylesheet.

**Written against `@milkdown/crepe` 7.21.3.** Its DOM, token names and
round-trip behaviour are version-specific: a bump re-verifies them.

- **Never import one of its own themes.** The global sheet loads its base CSS
  (~64 kB, far over the per-component budget) and maps every `--crepe-*` token
  onto the `--mat-sys-*` ones — that mapping is what makes its floating toolbars
  and menus follow the app palette and the light/dark scheme.
- **Never wrap it in Material toolbars or command buttons.** It owns its editing
  chrome; configure `features` / `featureConfigs` instead. Those flags gate
  behaviour, **not** bundle size: every feature is imported statically.
- It renders its DOM outside Angular, hence `ViewEncapsulation.None` and every
  rule hand-scoped under `.markdown-editor`; emulated encapsulation attributes
  never reach that DOM. Create and destroy share **one promise chain**, so
  overlapping mounts cannot leave a stale editor behind, and `setReadonly` is
  re-applied after `create()` because the control may already be disabled.
- **The write/emit loop needs all three guards** — the last-agreed markdown
  string, the `writing` flag, and the read-back of `getMarkdown()` immediately
  after `replaceAll`. The editor re-serialises what it round-trips (bullets,
  table padding, blank lines) and reports it *asynchronously*, so without the
  read-back a `patchValue` bounces back through `onChange`, marks the control
  dirty and rewrites what the user typed. The read-back reads as a redundant
  line; deleting it is how the loop comes back.
- It pulls in ProseMirror, CodeMirror and Vue: the routes that load it stay
  lazy, and the initial-bundle budget is what says when that stopped being true.

## 5. Lists: four states, one page, and no edit button

Loading → error with a Retry → empty → data, driven by `items`, `loading` and
`error` signals. `load()` is `protected` so Retry runs the same code path the
constructor did, and it resets both signals at both ends.

- **Click-to-edit is the only edit affordance.** The whole row or card navigates
  through `edit(item)`, carrying `tabindex="0"`, `(keydown.enter)` and an
  `[attr.aria-label]` naming the item; inner controls call
  `$event.stopPropagation()` first. **Never add an edit pencil** — it is what
  gets added *instead* of that trio, leaving a row no keyboard can reach.
- **Every list is one page of 100 filtered client-side** with `computed` over
  the filter signals; the filter controls never reach the server. Past 100 rows
  the view is quietly wrong, sooner for a list filtering a child collection
  (audits by task). **A paginator without server-side filtering is worse than
  neither** — it filters the page you are on and calls that a result set: if
  paging moves to the server, the filters move with it, in the same change.
- Deleting goes through the shared confirm dialog; only the confirmed branch
  calls the API.

## 6. Two files own the network, and they own it completely

`{Entity}Api` and the HTTP error interceptor. Nothing else touches a request.

- **Exactly five methods per CRUD entity**: `getAll(limit?, offset?)` returning
  `Paginated<T>`, `getById`, `create`, `update`, `delete`. Components read
  `.items` off the wrapper before storing it in a signal. A query-only endpoint
  gets only its query method — do not pad it to five.
- **Never inject `HttpClient` into a component**, or build a URL outside a
  service.
- **A sub-resource is a method on the parent's service, never a feature of its
  own**: `/meetings/{id}/contacts` becomes `MeetingApi.getContacts(id)`, typed
  with the child's generated model.
- **The interceptor already told the user the request failed.** A mutation
  subscribes with `next` only — success snackbar, then navigate — and a read
  handles `error` purely to flip the view into its error state. That missing
  `error` branch is a decision, not an omission: a failure snackbar in a
  component gives the user **two** toasts, and the second is the one the
  component guessed at — status 0 means "is the backend running?", not "could
  not save".
- **After a successful delete, call `load()` again** rather than splicing the
  local array: the list is a projection of the server, and the reload is what
  keeps totals and other writers' changes honest.

## 7. The constructor is the whole lifecycle

There is no `ngOnInit` here and no reason to write the first one. Dependencies
inject at field initialisation, data loads in the constructor, and teardown is
`DestroyRef.onDestroy` or `takeUntilDestroyed()` — **both need the injection
context the constructor is already inside**, and `ngOnInit` is where it is gone.

- **Visibility is a contract**: dependencies `private readonly`, everything a
  template reads `protected readonly`, `input()` / `output()` public `readonly`.
  A public method on a component is a mistake or an unextracted service.
- The app is **zoneless**. State outside a signal does not re-render, so plain
  fields (`selectedProject`, `pending{X}Id`) are a deliberate statement that no
  template reads them — and nothing a template reads may be one. In tests that
  means `await fixture.whenStable()`, never a `detectChanges()` loop.

## 8. Tests: six per service, and logic that left the component

`{entity}-api.spec.ts` covers **getAll, getById, create, update, delete and
error propagation** over `provideHttpClientTesting()`, with `verify()` in
`afterEach`. **The URL is asserted as a literal** (`/api/v1/{plural}`) — the
brittleness is the point: that literal is what fails when the base URL changes
shape.

- **Components have no fixtures.** Anything worth testing is exported as a free
  function beside the component and tested directly: if a component method
  deserves a test, it should not be a method.
- **A decision worth recording is worth one falsifiable test** — the base-URL
  spec asserts that requests reach a configured absolute host, the behaviour the
  decision exists for, not that a token holds a string.

## 9. Tokens, structure and naming

- **`--mat-sys-*` for colour, elevation, radius and typography.** No hex, no
  hand-written `box-shadow`, no magic radius in component CSS. **Data-driven
  colour is the exception**: `[style.background]="item.color"` is a value off
  the row, not a decision. Sticky elements take `--app-sticky-top`, defined
  once and different on mobile — `64px` is right on exactly one viewport.
- **`_theme-colors.scss` is generated** by the Material theme-colour schematic
  from the two source colours: regenerate it, never hand-edit it.
- **The per-component style budget (4 kB / 8 kB) is a detector, not a lint** —
  passing it means a Material component is being rebuilt by hand.

Files `{entity}.model.ts` / `-api.ts` / `-list.ts` / `-form.ts` — **no
`.component.ts` or `.service.ts` suffixes**; classes `{Entity}Api` /
`{Entity}List` / `{Entity}Form`; types `{Entity}Id`, `{Entity}`,
`Create{Entity}Request`, `Update{Entity}Request`. Three lazy routes per entity:
`{plural}`, `{plural}/new`, `{plural}/:id/edit`.

- **The drawer follows the information architecture, not the entity list.**
  Catalog entities live under the Settings subheader, and a **support-only
  entity gets a model, a service and a spec and no UI at all** — its rows are
  rendered by the parent's form. A list view for a catalog nobody manages is
  work the navigation then has to hide.
- `core/` is app-wide plumbing with no UI of its own, `shared/` is reusable UI.
  A component under `core/`, or a service with a template, is the smell.
