# Provider Profile Edit And Activation Design

## Context

The current `模型通道` page now supports:

- `官方通道`
- `混合中转`
- `传统中转`
- upstream protocol selection
- imported CCSwitch profiles

That broader capability exposed a UX problem in the current profile area:

- clicking a channel card may apply it immediately;
- selecting a profile also changes what is edited;
- saving a profile also applies it immediately;
- the UI does not clearly separate "the profile I am editing" from "the
  profile currently in use".

This creates overlapping state models:

- current active channel
- current active profile
- current editing profile

Users cannot reliably tell whether they are:

- only opening a profile to edit it,
- saving profile data without switching traffic,
- or actually enabling a different route for future Codex requests.

The result feels inconsistent even when each individual button is technically
working.

## Goals

- Make "which profile I am editing" obvious.
- Make "which profile is currently enabled" obvious.
- Let users edit a profile without automatically enabling it.
- Let users enable a profile explicitly after reviewing or saving it.
- Preserve inline card editing instead of reverting to a separate list/editor
  split.

## Non-Goals

- Do not redesign the page back into a left list plus right editor layout.
- Do not remove support for `官方通道`, `混合中转`, or `传统中转`.
- Do not change backend provider storage format.
- Do not add background autosave or draft persistence in this change.

## Product Decision

Profile selection, profile editing, and relay-mode switching should be separate
user actions.

Recommended rule:

- clicking a profile card means `make this the current selected/active profile`
- clicking `编辑` means `open this profile for editing`
- clicking a top-level mode card means `use the current selected profile with
  this relay mode now`

These actions are related, but they must not be treated as the same event.

This is the correct tradeoff because the current confusion is not visual polish;
it comes from coupling three different intents too tightly:

- which config is selected
- which config is being edited
- which relay mode is currently enabled

## Interaction Model

The page should use three explicit state concepts:

1. `activeProfileId`
   The profile currently selected to be used.

2. `editingProfileId`
   The profile card currently expanded for editing.

3. `activeMode`
   The current enabled route mode:
   - `official`
   - `hybridApi`
   - `api` (shown as `传统中转`)

`activeProfileId` and `editingProfileId` may be different.

### Core Rules

- Clicking a profile card selects that profile as the current active profile.
- Clicking a profile card does not open inline editing by itself.
- Clicking `编辑` opens that profile for editing.
- `保存配置` saves only the profile data.
- `混合中转` and `传统中转` switch how the current active profile is used.
- `官方通道` clears custom routing entirely.
- The current active profile should remain visibly marked even when another
  profile is open for editing.

## Card States

Each card should be able to communicate two independent ideas:

- whether this card is the currently selected/active profile
- whether this card is expanded for editing

Suggested visible states:

- `当前配置`
- `正在编辑`

Examples:

- active + editing
  - the user is editing the currently selected profile
- editing only
  - the user is changing a profile that is not the current selected profile
- active only
  - the currently selected profile is collapsed

## Page Structure

Keep one unified `配置档` list.

Inside each profile card:

- header:
  - profile name
  - profile summary: protocol, base URL
  - state badges
  - `编辑` button
- expanded form:
  - `配置名称`
  - `Base URL`
  - `API Key`
  - `上游协议`
- actions:
  - `保存配置`
  - `删除配置`

`CCSwitch 配置` import remains a compact row above the profile list.

The list-item body outside the `编辑` button acts as the "select / use this
configuration" hit area.

## Channel Controls

The top-level channel cards should remain immediate route switches, but they
must no longer imply profile editing.

### Official Channel

`官方通道` may remain a direct action because it is not tied to a specific
editable API profile. It clears the custom provider route explicitly.

### API-Backed Channels

`混合中转` and `传统中转` should apply immediately when clicked at the top
level.

Instead:

- they use the current `activeProfileId`
- they change only the route mode
- they do not open editing
- they do not save profile fields

This keeps the interaction model clear:

- choose a configuration below
- choose a route mode above

## Save Behavior

`保存配置` should:

1. validate the expanded card fields
2. save the profile through `save_provider_profile`
3. refresh local snapshot state
4. keep the same card open for editing
5. not call `apply_provider`

If save succeeds, the UI should confirm that the profile was updated, but it
must not imply that traffic was switched.

## Profile Selection Behavior

Clicking the card body outside the `编辑` button should:

1. make that profile the current selected/active profile
2. refresh snapshot state if needed
3. not open editing automatically
4. not change `activeMode` by itself

This means users can first choose which config is current, then switch between
`混合中转` and `传统中转` at the top without ambiguity.

## New Profiles

Creating a new profile should still open an inline editable card.

For a newly created profile:

- `保存配置` creates it
- the new profile does not automatically change the route mode
- the new profile does not automatically apply `混合中转` or `传统中转`
- after save, the user may select that profile and then choose a top-level mode

This is important because auto-switching after create/save would recreate the
same confusion this design is trying to remove.

## Existing Design Alignment

This design intentionally updates parts of earlier provider behavior.

Still true:

- editing happens directly inside the selected profile card
- provider sync remains separate maintenance
- profile import remains additive and non-activating by default

What changes:

- visible `无账号` naming changes to `传统中转`
- `保存配置` no longer implies immediate activation
- the top-level route cards remain direct actions
- selecting a profile and editing a profile are no longer the same click

This means implementation should update or supersede any older spec wording that
still says:

- visible no-login wording should remain `无账号`
- saving a profile automatically applies it
- clicking a profile must immediately open it for editing

## Acceptance Criteria

- Visible label is `传统中转`, not `无账号`.
- Clicking a profile card selects that specific profile as current.
- Clicking `编辑` opens that specific profile for editing.
- Selecting a profile does not open editing automatically.
- The active profile and the editing profile can be different and are visibly
  distinguishable.
- Saving a profile does not automatically switch the current live route.
- Top-level `混合中转` and `传统中转` apply immediately to the current selected
  profile.
- Top-level mode switching does not implicitly save profile edits.
- New profiles are not auto-activated just because they were saved.
