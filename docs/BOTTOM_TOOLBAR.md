# Bottom Toolbar

![Main page toolbar - both rows above the pattern cards](images/toolbar.png)

## What The Bottom Toolbar Is For

The bottom toolbar is the performance and device-control strip on the Control page.

It handles the parts of the workflow that affect playback, MIDI connection, timing, and live communication with the TD-3. Pattern writing, pattern editing, import, export, randomization, and bank work happen elsewhere in the interface. The bottom toolbar is mainly about answering four practical questions:

- Is the TD-3 connected?
- What clock source is the TD-3 using?
- Is the pattern playing?
- What tempo is being used?

## MIDI Connection Button

The round button on the left connects or disconnects the TD-3 MIDI session.

When the app is disconnected, the button shows a warning-style icon and the label reads `DISCONNECTED`.

![Bottom toolbar showing the disconnected MIDI state](images/disconnected.png)

Clicking it asks the app to connect to the TD-3. When connection succeeds, the app can send patterns, control transport, update tempo, and use preview workflows that depend on the hardware.

Clicking it again disconnects the MIDI session.

The color also matters:

- Grey or red means the app is not connected to the TD-3.
- Green means the TD-3 is connected and ready for USB-controlled playback.
- Yellow means the TD-3 is connected, but its sync source is not USB, so the app may not be able to drive playback from the toolbar.

If the toolbar says the TD-3 is disconnected, editing and file work can still be useful, but hardware playback and direct device writes are unavailable.

## Sync Source Buttons

The small vertical column marked `INT`, `USB`, `DIN`, and `TRIG` controls the TD-3 clock source.

These buttons tell the TD-3 where its timing should come from:

- `INT` uses the TD-3 internal clock.
- `USB` lets the app drive playback timing over USB.
- `DIN` uses external MIDI DIN sync.
- `TRIG` uses trigger sync.

For normal use with this app, choose `USB`. That is the mode where the Play button and BPM control are intended to drive the TD-3 from the web interface.

Use the other sync sources when the TD-3 should follow another piece of gear instead of the app. For example, `DIN` can be useful when another sequencer or drum machine is the master clock.

The sync buttons are disabled while no TD-3 is connected. The active source is highlighted when the app can read it from the device.

## Play And Stop

The large round `PLAY / STOP` button starts and stops TD-3 playback.

When stopped, the button shows a play icon. Click it to start playback at the current BPM.

When playing, the button changes to a stop icon. Click it again to stop the TD-3.

On a single focused pattern, playback loops that pattern.

When the timeline contains multiple pattern slots, playback follows the timeline order. The app prepares the next pattern before the TD-3 reaches the loop point so the hardware can move into the next pattern cleanly.

Playback requires a MIDI connection. If the TD-3 is disconnected, the status message will ask you to connect MIDI first.

## Live Update

The `LIVE` button controls whether pattern changes are sent automatically to the configured scratch slot while you work.

When Live Update is on, edits can be pushed to the TD-3 scratch slot shortly after you make them. This is useful when you want to hear changes on the hardware without manually saving after every edit.

When Live Update is off, edits stay in the app until you explicitly send, save, preview, or push them through another control.

Live Update is powerful because it makes the TD-3 feel connected to the editor in real time. It should also be used with awareness: the scratch slot is meant to be overwritten during live work.

## BPM Display

The large number in the bottom toolbar is the current BPM.

This tempo is used for app-driven playback and preview timing. When the TD-3 is connected and playing from USB sync, BPM changes are sent to the device.

The displayed value updates as you change the tempo.

## BPM Knob

The round BPM knob changes the tempo.

You can adjust it in two ways:

- Scroll the mouse wheel over the knob to move the tempo up or down by one BPM.
- Click and drag the knob vertically for faster changes.

If playback is already running, the app updates the playback timer and sends the new BPM to the TD-3 when possible.

## Status Message

The text area on the right side of the bottom toolbar shows short status messages.

It reports what just happened, such as:

- connected or disconnected MIDI
- playback started or stopped
- BPM update errors
- live-send results
- timeline playback position
- device communication errors

This message area is not a full log. It is a quick feedback line so you can tell whether the last action succeeded, failed, or needs attention.

## Recommended Use

For the most common workflow:

1. Connect the TD-3 with the MIDI button.
2. Set the sync source to `USB`.
3. Choose a BPM with the knob.
4. Turn `LIVE` on if you want edits to reach the scratch slot automatically.
5. Press `PLAY / STOP` to start and stop playback.
6. Watch the status message when something does not behave as expected.

The bottom toolbar is designed to keep the hardware side visible while the rest of the page focuses on pattern creation.

