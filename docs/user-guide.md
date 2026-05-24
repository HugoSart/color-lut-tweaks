# User Guide

## FAQ

### Why is the app not working or don't I see any color difference?

There are a few Windows and GPU driver settings that will prevent the application from working if enabled.
For short:
- Disable Windows "Auto Color Management"
- For NVIDIA: Disable "System > Displays > Color > Color Accuracy Mode > Override to reference mode"
- For AMD: Disable "Gaming > Display > Custom Color > Color Temperature Control"

### Why is the preset being applied to the wrong monitor?

The pre-defined presets by default assume that the monitor being adjusted is always the first monitor. In case you have
multiple monitors, and you desire to apply the preset for a different one, click "Edit" and change the "device" 
id manually. After that, click "Reload" to re-apply the preset.

### How do I report an issue?

To report a new issue, create a new [Issue](https://github.com/HugoSart/color-lut-tweaks/issues/new), and in the right
panel under `Labels` tag, and select `bug`.

Optionally, zip and upload the error logs that are in the `logs` folder inside the application folder using the
`Paste, drop, or click to add files` button on the bottom of the issue page.

### How do I suggest a new feature?

To suggest a new feature, create a new [Issue](https://github.com/HugoSart/color-lut-tweaks/issues/new), 
and in the right panel under `Labels` tag, and select `enhancement`.


### How do I ask a question or seek help?

To request help or ask a question, create a new [Issue](https://github.com/HugoSart/color-lut-tweaks/issues/new),
and in the right panel under `Labels` tag, and select `help wanted` or `question`.
