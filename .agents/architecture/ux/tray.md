# UX: Tray

User experience and design notes for the system tray UI.

The system tray UI is represented using Markdown syntax below. Use the **Components** section to know what each UI
component should a Markdown item represent.

## Components

- `Lorem Ipsum`: Clickable text
- `Lorem ipsum > Lorem ipsum`: Subgroup
- `_Lorem Ipsum_`: Grayed out text / label
- `[ ] Lorem ipsum`: Checkbox
- `---`: Divider

## Design
```text
- "_Devices_"
- "_<monitor-index>: <human-readable-monitor-name> (<dynamic-range-mode>)_"
  - for-each: monitor found in monitor list
---
- "_Color Adjustments_"
- "Presets > <preset-name>"
  - for-each: preset found in presets list
  - on-click: apply the preset.
- "[ ] Override > Ignore SDR adjustments"
  - on-click: toggle ignore SDR adjustments.
- "[ ] Override > Ignore HDR adjustments"
  - on-click: toggle ignore HDR adjustments.
- "[ ] Override > Ignore Windows adjustments"
  - on-click: toggle ignore HDR adjustments. 
- "Edit"
  - on-click: open the config file in the default text editor.
- "[ ] Enabled"
  - on-click`: enable / disable the color adjustments.
- "[ ] Force"
  - `on-click`: enable / disable force mode (i.e., automatically reapply color adjustments if change is detected)
- "Reload"
  - on-click`: reload the color adjustments instantly once. 
---
- "Update available" if there is an update available; else "Check for updates"
  - on-click: redirect to GitHub Releases page.
- "Open In Explorer"
  - on-click: open the installation folder in Explorer.
- [ ] Start with Windows
  - on-click: toggle start with Windows.
- "Help > Read this if the app seems to be not working"
  - on-click: redirect to https://github.com/HugoSart/color-lut-tweaks/blob/main/docs/user-guide.md#why-is-the-app-not-working-or-dont-I-see-any-color-difference
- "Help > Report issue"
  - on-click: redirect to https://github.com/HugoSart/color-lut-tweaks/blob/main/docs/user-guide.md#how-do-I-report-an-issue
- "Help > Feature request"
  - on-click: redirect to https://github.com/HugoSart/color-lut-tweaks/blob/main/docs/user-guide.md#how-do-I-suggest-a-new-feature
- "Quit"
  - on-click: quit the application. 
```