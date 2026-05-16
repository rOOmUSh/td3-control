# Bank Buttons

## What This Page Covers

The Bank page is the app's persistent pattern library. It has more buttons than any other page because it combines browsing, importing, tagging, snapshot management, duplicate review, related-group review, compare tools, merge planning, device audition, and Control-page handoff.

This guide explains what each visible button does from a user's point of view.

## Global Navigation

These buttons live in the top bar.

| Button | What it does |
| --- | --- |
| `CONTROL` | Opens the main multipattern Control page. |
| `PROGRESSION` | Opens the progression generator page. |
| Settings icon | Opens the Settings page. |

These are page navigation buttons. They do not change the Bank catalog by themselves.

## Left Library Sidebar

The left sidebar buttons choose which Bank section is shown.

| Button | What it shows |
| --- | --- |
| `All Items` | The full item library, subject to search and filters. |
| `Snapshots` | Saved 64-slot bank snapshots. |
| `Imported Folders` | Import and scan batches. |
| `Related Groups` | Groups that share scale, root, rhythm, analyzer relation, or progression family. |
| `Duplicates` | Exact and near duplicate clusters. |
| `Favorites` | Items marked with the favorite star. |
| `Needs Review` | Items whose analysis status requires review. |
| `Failed Imports` | Failed file entries across import batches. |

Switching sections clears the current item selection and applies the section's focused filter. For example, `Favorites` turns on the favorites-only filter, and `Duplicates` turns on the duplicates-only filter.

## Bottom Bar

The Bank bottom bar is only for MIDI connection and audition BPM.

| Control | What it does |
| --- | --- |
| MIDI round button | Connects to the TD-3 when disconnected. Disconnects when connected. |
| BPM knob | Changes the BPM used by Bank play buttons. Use the mouse wheel or drag vertically. |

There is no global play button in the Bank. Each pattern has its own play button because the page needs to know which pattern to audition.

## Shared Play Buttons

The play button appears on item cards, table rows, drawer actions, snapshot slots, duplicate members, related representatives, and imported entries that resolved to a Bank item.

| State | Meaning |
| --- | --- |
| Play icon | Uploads that item to the configured scratch slot and starts TD-3 playback at the Bank BPM. |
| Stop icon | Stops the currently playing item. |

Only one Bank item is tracked as playing at a time. Starting another item updates all visible play buttons that refer to the same Bank item.

## Main Toolbar

The toolbar sits above the main Bank content.

| Button | What it does |
| --- | --- |
| `FILTER` | Opens the filter popover. |
| `FOLDER SCAN` | Opens the folder scan modal. |
| `IMPORT` | Opens the direct file import modal. |
| `NEW SNAPSHOT` | Creates a new empty manual snapshot after asking for a name and optional description. |
| `COMPARE` | Compares exactly two selected items. |
| `MERGE` | Opens the snapshot merge planner. |
| `ADD TO CONTROL` | Appends selected Bank items to the Control page. |
| `CARDS` | Shows items as cards. |
| `TABLE` | Shows items as a sortable table. |
| `DENSE` | Toggles compact visual spacing. |
| `CLEAR` | Clears the current item selection. |

`COMPARE` requires exactly two selected items. `ADD TO CONTROL` requires at least one selected item. If more than ten items are sent to Control, the app asks for confirmation because Control is capped at 64 patterns.

## Search Box

The search box is not a button, but it is one of the main Bank controls.

It accepts plain text and structured tokens such as:

- `tag:acid`
- `scale:phrygian`
- `root:D`
- `slot:G2P4B`
- `snapshot:"April backup"`
- `favorite`
- `format:seq`

Typing updates the active filter after a short delay.

## Filter Popover

`FILTER` opens a popover with dropdowns, text inputs, and checkboxes.

Filter controls include:

- Format
- Source kind
- Favorites only
- Include archived
- Duplicates only
- Related only
- Failed imports only
- Needs review only
- Snapshot
- Slot key
- Scale name
- Root
- Tag
- Date from
- Date to

| Button | What it does |
| --- | --- |
| `RESET` | Clears the filter back to the default Bank filter. |
| `CLOSE` | Closes the filter popover without changing current values. |

Most filter values apply immediately when changed.

## Folder Scan Modal

`FOLDER SCAN` opens this modal.

| Button or control | What it does |
| --- | --- |
| `BROWSE` | Opens the platform folder picker and fills the folder path. |
| `Recurse into subfolders` | Includes subfolders during the scan when checked. |
| `Scan` | Starts scanning the chosen folder for supported pattern and bank files. |
| `Cancel` | Closes the modal. |
| Close icon | Closes the modal. |

The modal shows live scan progress while files are found and parsed. The scan result is stored as an import batch.

## Import Files Modal

`IMPORT` opens this modal.

| Button | What it does |
| --- | --- |
| `Import` | Imports the absolute file paths listed in the text area, one path per line. |
| `Cancel` | Closes the modal. |
| Close icon | Closes the modal. |

Duplicate files can be skipped by the import pipeline. Unsupported and failed files are recorded in the import batch.

## New Snapshot Prompts

`NEW SNAPSHOT` uses two prompt modals.

| Button | What it does |
| --- | --- |
| `Next` | Accepts the snapshot name and moves to the description prompt. |
| `Create` | Creates the snapshot using the entered name and optional description. |
| `Cancel` | Cancels the prompt. |
| Close icon | Closes the prompt. |

A new snapshot starts as a manual snapshot record. It does not automatically write to the TD-3.

## Item Cards

Card view shows each Bank item as a card.

| Button or action | What it does |
| --- | --- |
| Play button | Auditions the item through the TD-3 scratch slot. |
| Open details icon | Opens the right-side details drawer for the item. |
| Add tag icon | Opens a tag prompt for that item. |
| Toggle selection icon | Selects or deselects the item. Shift-click supports range selection. |
| Favorite star | Toggles favorite state. |
| Tag remove `x` | Removes that tag from the item. |
| `ADD TO CONTROL` | Appends that one item to the Control page without selecting it first. |
| `Delete` | Opens a confirmation to delete the item from the Bank database. |
| Card click | Selects or deselects the card. |
| Card double-click | Opens the details drawer. |

Deleting an item removes the Bank database record and tag links. It does not delete source files and does not touch the TD-3.

## Table View

Table view is for dense browsing and bulk work.

| Button or action | What it does |
| --- | --- |
| Header checkbox | Selects all visible rows, or clears the visible selection. |
| Row checkbox | Selects or deselects one row. Shift-click supports range selection. |
| Row play button | Auditions that item on the TD-3. |
| Column header | Sorts by that column. Shift-click adds the column to multi-column sort. |
| Favorite star | Toggles favorite state. |
| More icon | Opens the row action menu. |
| Row click | Selects or deselects the row. |
| Row double-click | Opens the details drawer. |

The row action menu contains:

| Menu button | What it does |
| --- | --- |
| `Open details` | Opens the details drawer for that row. |
| `Copy Path` | Copies the source path to the clipboard. |
| `Copy Slot` | Copies the slot key to the clipboard. |
| `Copy ID` | Copies the Bank item ID to the clipboard. |

## Table Bulk Bar

When one or more rows are selected in table view, a bulk bar appears.

| Button | What it does |
| --- | --- |
| `BULK TAG` | Opens the bulk tag modal. |
| `FAV` | Marks all selected rows as favorites. |
| `ARCHIVE` | Opens a confirmation, then archives all selected rows. |
| `ADD TO SNAPSHOT` | Currently shows that this action is not available from the table bulk bar. |
| `QUEUE FOR MERGE` | Currently shows that this action is not available from the table bulk bar. |
| `CLEAR` | Clears the table selection. |

## Bulk Tag Modal

`BULK TAG` opens a modal for adding and removing tags across selected items.

| Control | What it does |
| --- | --- |
| Add tags checkboxes | Tags to add to every selected item. |
| Extra new tags input | New comma-separated tag labels to create and add. |
| Remove tags checkboxes | Tags to remove from selected items. |
| `Apply` | Applies the selected add and remove operations. |
| `Cancel` | Closes the modal. |
| Close icon | Closes the modal. |

At least one add or remove operation is required before `Apply` can succeed.

## Details Drawer

The details drawer opens from a card, table row, duplicate member, related group action, or snapshot slot.

| Button | What it does |
| --- | --- |
| Floating close tab | Closes the drawer. |
| Header close icon | Closes the drawer. |
| `PLAY` or `STOP` | Auditions or stops this item through the TD-3. |
| `FAVORITE` | Toggles favorite state. |
| `ARCHIVE` | Opens a confirmation and toggles archived state. |
| `COMPARE` | Compares this item with exactly one other selected item. |
| `TAG` | Opens a tag prompt. |
| `ADD TO SNAPSHOT` | Opens the Add to Snapshot modal for this item. |
| `COPY META` | Copies the full item metadata JSON to the clipboard. |
| `OPEN LOCATION` | Tries to open the source path, or copies the path if the browser blocks it. |
| Tag remove `x` | Removes that tag from the item. |
| Tag autocomplete suggestions | Adds the clicked or keyboard-selected tag. |
| `VIEW CLUSTER` | Jumps to the Duplicates section for this item's cluster. |
| Cluster `COMPARE` | Compares this item with another member of its duplicate cluster. |
| `RAW / TECHNICAL` | Expands or collapses raw item metadata. |

The drawer is metadata-heavy, but most actions still affect only the Bank database unless the action is a play button.

## Add To Snapshot Modal

The drawer's `ADD TO SNAPSHOT` button opens this modal.

| Control | What it does |
| --- | --- |
| Snapshot dropdown | Chooses the target snapshot. |
| Slot dropdown | Chooses a specific empty slot, or leaves the app to use the first free slot. |
| `Add` | Adds the item to the selected snapshot and slot. |
| `Cancel` | Closes the modal. |
| Close icon | Closes the modal. |

If no snapshot exists, the app creates a new timestamped snapshot.

## Empty Search Results

When filters hide all matching items, the empty-state panel can show:

| Button | What it does |
| --- | --- |
| `CLEAR FILTERS` | Resets filters and clears the visible search input. |

This appears only when the library has items but the active filters exclude them.

## Snapshots List

The Snapshots section has a list mode and a detail mode.

In list mode:

| Button or action | What it does |
| --- | --- |
| `SYNC BACKUPS` | Imports backup zip snapshots from the configured backup directory. |
| Snapshot card click | Opens the snapshot detail grid. |
| Snapshot card Enter or Space | Opens the snapshot detail grid. |
| Snapshot `Delete` | Opens a confirmation to delete that snapshot record and its slot mappings. |

Deleting a snapshot does not delete original source files and does not write to the TD-3.

## Snapshot Detail Header

In snapshot detail mode:

| Button or control | What it does |
| --- | --- |
| `BACK` | Returns to the snapshot list. |
| Snapshot name field | Rename the snapshot when the field changes. |
| Description field | Saves the description when the field changes. |
| `PIN` | Pins the snapshot so it sorts ahead of unpinned snapshots. |
| `UNPIN` | Removes pinned status. |
| `RENAME` | Focuses and selects the snapshot name field. |
| `COMPARE WITH...` | Opens a picker for choosing another snapshot to compare against. |
| `MERGE FROM...` | Opens the merge planner with this snapshot pre-filled as the target. |
| `ADD TO CONTROL` | Appends selected occupied slots to the Control page. |
| `DELETE` | Deletes selected slots from this snapshot after confirmation. |
| `EXPORT` | Opens the export format dropdown for selected slots. |

`ADD TO CONTROL`, `DELETE`, and `EXPORT` operate on selected snapshot slots, not necessarily the whole snapshot.

## Snapshot Slot Grid

Snapshot slots are button-like cells in a 64-slot grid.

| Action | What it does |
| --- | --- |
| Click occupied slot | Selects or deselects the slot for export, delete, or add to Control. |
| Double-click occupied slot | Opens the item drawer for that slot. |
| Click empty slot | Opens an empty-slot placeholder. |
| Slot play button | Auditions the linked Bank item through the TD-3. |
| Drag occupied slot | Starts a snapshot slot move. |
| Drop onto empty slot | Moves the item into that empty slot. |
| Drop onto occupied slot | Swaps the two occupied slots. |

Slot deletion clears slots inside the snapshot. It does not delete the underlying Bank items.

## Snapshot Export Dropdown

Hovering `EXPORT` opens the snapshot export dropdown.

| Control | What it does |
| --- | --- |
| Format checkboxes | Choose one or more export formats. |
| `EXPORT` | Opens the folder picker and writes each selected slot in each checked format. |

Formats offered are:

- `MIDI`
- `STEPS.TXT`
- `SEQ`
- `PAT`
- `RBS`
- `TOML`
- `JSON`

The export button is enabled only when at least one format is checked and at least one snapshot slot is selected. The chosen formats are remembered in the browser.

## Delete Snapshot Slots Modal

The snapshot detail `DELETE` button opens a confirmation modal when slots are selected.

| Button | What it does |
| --- | --- |
| `CONFIRM` | Clears the selected slots from the snapshot. |
| `CANCEL` | Closes the modal without deleting slots. |
| Close icon | Closes the modal. |

The snapshot remains a 64-cell grid. Deleted slots become empty placeholders.

## Compare Snapshot Picker

`COMPARE WITH...` opens a picker listing other snapshots.

| Button | What it does |
| --- | --- |
| Snapshot row | Runs snapshot compare against the chosen snapshot. |
| `CANCEL` | Closes the picker. |

The current snapshot is the source side. The selected snapshot is the comparison target.

## Pattern Compare Modal

`COMPARE` from the toolbar, drawer, duplicate cluster, or related group opens Pattern Compare.

| Button or section | What it does |
| --- | --- |
| `Raw JSON` | Expands the raw compare report. |
| `CLOSE` | Closes the compare modal. |
| Close by backdrop or Escape | Closes the modal. |

The modal shows duplicate score, relatedness score, byte identity, rhythm identity, field diff counts, active-step status, triplet status, and a 16-step diff grid.

## Snapshot Compare Modal

Snapshot compare shows a slot-by-slot result for two snapshots.

| Button or section | What it does |
| --- | --- |
| `Raw JSON` | Expands the raw snapshot compare report. |
| `CLOSE` | Closes the compare modal. |

The summary counts identical, changed, added, removed, and empty slots.

## Merge Planner

`MERGE`, `MERGE FROM...`, or related snapshot flows can open the merge planner.

The merge planner does not write to the TD-3. It builds and downloads a JSON plan.

| Control | What it does |
| --- | --- |
| Source snapshot dropdown | Chooses the source snapshot. |
| Target snapshot dropdown | Chooses the target snapshot. |
| Slot checkboxes | Select which slot operations should be included. |
| `All Different` | Selects only slots whose source and target contents differ. |
| `Default (Diff + TargetOnly)` | Restores the default merge selection. |
| `Everything` | Selects all slot states. |
| `Clear` | Clears the merge selection. |
| Overwrite confirmation checkbox | Required before downloading a plan that overwrites or clears target slots. |
| `Download Plan JSON` | Builds the final merge plan and downloads it as JSON. |
| `Cancel` | Closes the modal. |
| Close icon | Closes the modal. |

The preview panel groups planned operations into copy, clear, keep, and skip buckets before you download the plan.

## Related Groups

The Related Groups section has kind chips at the top.

| Chip | What it filters |
| --- | --- |
| `All` | Shows every related group kind. |
| `Scale` | Shows same-scale groups. |
| `Root` | Shows same-root groups. |
| `Rhythm` | Shows same-rhythm groups. |
| `Analyzer` | Shows analyzer-related groups. |
| `Progression` | Shows progression-family groups. |

Each related group card can include representative play buttons and group actions.

| Button | What it does |
| --- | --- |
| Representative play button | Auditions that representative item. |
| `Open Group` | Shows the group's items in the All Items view. |
| `Compare 2` | Compares the first two items in the group. |
| `Add to Snapshot` | Opens the Add Group to Snapshot modal. |
| `Progression Seed` | Currently reports that this action is not available from this view. |

## Add Group To Snapshot Modal

`Add to Snapshot` in a related group opens this modal.

| Control | What it does |
| --- | --- |
| New snapshot name | Creates a new snapshot record with that name. |
| Existing snapshot dropdown | Selects an existing snapshot target. |
| `Save` | Creates or selects the snapshot target. |
| `Cancel` | Closes the modal. |
| Close icon | Closes the modal. |

This action creates or selects the snapshot record only. It does not populate snapshot slots from the group.

## Duplicates Section

The Duplicates section shows exact and near duplicate clusters.

| Button | What it does |
| --- | --- |
| `REFRESH` | Reloads the Bank data and duplicate clusters. |
| Member play button | Auditions that duplicate member. |
| Member chip | Opens that item in the drawer. |
| `COMPARE` | Compares the cluster representative with another member. |
| `OPEN REPRESENTATIVE` | Opens the representative item in the drawer. |
| `FILTER TO CLUSTER` | Shows only that cluster's items in the All Items view. |

The representative is the item the duplicate view suggests keeping, but the button does not delete anything.

## Imported Folders

The Imported Folders section shows scan and import batches.

In the batch list:

| Button or action | What it does |
| --- | --- |
| Batch card click | Opens batch details. |
| `View details` | Opens batch details. |
| `Retry failed` | Retries failed files from that batch. |
| `Delete` | Opens a confirmation to delete the import batch record. |

In batch detail:

| Button | What it does |
| --- | --- |
| `BACK` | Returns to the import batch list. |
| `Retry failed` | Retries failed files for the open batch. |
| `Delete` | Opens a confirmation to delete the open batch. |
| Entry play button | Auditions an imported entry that became a Bank item. |
| Entry row actions icon | Opens the entry action menu. |

Entry action menu:

| Menu button | What it does |
| --- | --- |
| `Retry batch` | Retries failed files in that entry's batch. |
| `Copy Path` | Copies the file path to the clipboard. |
| `Copy Error` | Copies the recorded import error. |
| `Open item in drawer` | Opens the linked Bank item, if the entry has one. |

Deleting an import batch removes the batch record plus items and snapshots exclusively owned by that batch. It does not delete original files on disk.

## Failed Imports

Failed Imports reuses the import entry table across all batches.

| Button | What it does |
| --- | --- |
| Entry row actions icon | Opens retry and copy actions for that failed entry. |
| `Retry batch` | Retries the whole batch that contains the failed entry. |
| `Copy Path` | Copies the failed file path. |
| `Copy Error` | Copies the error text. |

If a failed entry later imports successfully, it can gain a play button and an item drawer link after the library refreshes.

## Confirmation Buttons

Many Bank actions use custom Bank modals instead of browser dialogs.

Common confirmation buttons:

| Button | Meaning |
| --- | --- |
| `Confirm` | Accepts a destructive or important action. |
| `Delete` | Confirms deleting a Bank item, snapshot, or import batch. |
| `Archive` | Confirms archiving selected items. |
| `Add` | Confirms adding many patterns to Control or adding an item to a snapshot. |
| `Cancel` | Closes without applying the action. |
| Close icon | Closes without applying the action. |

When a destructive action says source files or the TD-3 are not touched, it means the action affects Bank catalog records only.

## Quick Safety Notes

- Play buttons use the TD-3 scratch slot and transport.
- Import, scan, tag, favorite, archive, compare, and merge planning do not write to the TD-3.
- Delete item removes a Bank item record, not the original source file.
- Delete snapshot removes the snapshot record and slot mappings, not every source file.
- Delete snapshot slots clears cells inside one snapshot, not the underlying Bank items.
- Merge planner downloads a plan JSON. It does not perform a device write.
- Add To Control appends patterns to the Control workspace. It does not write to the TD-3 by itself.
