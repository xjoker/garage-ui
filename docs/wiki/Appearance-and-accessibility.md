# Appearance and accessibility

Open **Settings → Appearance** to customize this browser's console.

## Choose a theme or language

Choose **Light**, **Dark**, or **System** under **Theme**. **System** follows the operating system's preferred color scheme. The theme selection is saved in the browser and synchronizes across tabs that use the same storage.

Choose **English** or **Chinese** under **Language**; the Chinese UI is Simplified Chinese. Language changes labels and localized number/date presentation. Resource IDs, object keys, and stored data stay intact. Capacity and timestamp displays offer exact values where precision matters.

In **Settings**, **About** shows the UI version and Garage version/capability information. Use those values when reporting a problem, along with the exact `/healthz` commit described in [Deployment](Deployment.md#verify-the-deployed-source-revision).

## Navigate with a keyboard

- Use Tab and Shift+Tab to move through controls; the focused control has a visible focus indicator.
- Use **Skip to content** to move past the navigation.
- On a detail tab bar, Left/Right move between tabs; Home/End choose the first/last tab.
- In an object preview, Left/Right move between objects in the loaded directory.
- Use Escape to close the mobile navigation or a dismissible dialog. Confirmations return focus to their triggering control.

Buttons and file inputs retain their native keyboard behavior. Radix tabs, selects, menus, and dialogs provide keyboard interaction for their roles. Destructive dialogs show the affected resource and may require typing its name; finish that field before confirming.

## Use a narrow screen

Use **Menu** to open the **Sections** dialog and **Close navigation** or Escape to dismiss it. Select a section to navigate. The account button in the header opens account and server status details plus the sign-out control. Resource tables adapt to the available width; scroll a table region where needed to read long identifiers or audit fields. Open row **Actions** for controls that are grouped on compact layouts. The main Radix controls on mobile have at least 44-pixel targets, and text inputs use 16-pixel text to avoid browser zoom while typing.

Unsaved forms warn before navigation. Active uploads also warn that unfinished work will be cancelled if you leave. Finish or cancel those changes deliberately rather than dismissing the warning automatically.

If a page displays a loading or unavailable state, wait for it to resolve or use its **Retry** control. Retry preserves the task context; repeated failures need the checks in [Troubleshooting](Troubleshooting.md).

## Read status and finish actions

**Overview → Refresh** updates cluster health, capacity, and activity together. Existing values remain visible during a refresh. Activity counters are cumulative, not rates, and available capacity is reported by Garage rather than inferred from a usage chart.

Page headers contain the title and main action. Filters sit above the resource list; detail tabs keep related settings together. Dialog footers separate completion and cancellation controls from the fields. Dangerous actions continue to describe their effect and require the existing confirmation.

In a secret dialog, **Show secret** reveals the value and **Hide secret** masks it again. **Copy secret** reports whether copying succeeded; if the browser blocks it, the dialog selects the value for manual copying. Closing the dialog removes the secret from the page.

The interface uses the Radix Slate scale for surfaces and text, Orange for primary actions, and semantic colors for status. Typography uses 12, 14, 16, and 20 pixels. Separators and control borders use 0.5-pixel strokes; keyboard focus indicators remain distinct. Loading indicators animate only when reduced motion is not requested.
