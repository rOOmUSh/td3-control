// Shared 64-slot snapshot grid builder.
//
// Used by two surfaces:
//
//   1. Bank → Snapshots detail view (`bank/bank-snapshots.js`). Slots come
//      from `/api/bank/snapshots/:id`, each occupied slot is linked to a
//      LibraryItem by `item_id`, and the play button is the bank's global
//      `makePlayButton(item_id)` - it bus-subscribes to
//      `/api/bank/items/:id/play` / `/api/bank/playing` so every surface
//      that shows the same item stays in sync.
//
//   2. Single-pattern page → sqs/rbs import picker (`import-bank-picker.js`).
//      Slots come from `/api/pattern/parse-bank` and carry a raw `pattern`
//      payload instead of an `item_id`. The play button audits via
//      `/api/pattern/play-preview`, scoped to the picker modal's local
//      play state.
//
// Both surfaces render the same DOM shape (`.snapshot-grid-wrap` →
// `.snapshot-grid` → `.snapshot-slot`*64) so the existing CSS in
// `ui/css/bank.css` styles both without duplication. The module is
// purposely dependency-free: no bank state imports, no toast, no play
// infrastructure - everything surface-specific comes in via `callbacks`.
//
// Slot shape expected by this module (superset of bank's SnapshotSlotView
// and the parse-bank SlotView):
//
//   {
//     slot_key:     "G1-P1A",     // canonical dashed key
//     empty:        bool,
//     display_name: string | null,
//     changed:      bool | undefined,   // optional compare marker
//     duplicate:    bool | undefined,   // optional compare marker
//     // surface-specific fields (item_id, pattern, …) are reached only
//     // through the provided callbacks - this module never touches them.
//   }

/**
 * Build a 64-slot snapshot grid DOM tree.
 *
 * @param {Array<object>} slots  Ordered canonical G1-P1A..G4-P8B.
 * @param {object} callbacks
 *   isSelected(slot)        -> bool                 (default: always false)
 *   onClick(slot, ev)       -> void                 (default: no-op)
 *   onDblClick(slot, ev)    -> void                 (default: no-op)
 *   onKeyActivate(slot, ev) -> void                 (default: fall back to onClick)
 *   makePlayButton(slot)    -> HTMLElement | null   (default: null, no play button)
 *   showLegend              -> bool                 (default: true)
 *   dragDrop                -> { isDraggable(slot)?, isDropTarget(fromKey, slot)?, onDrop(fromKey, toSlot) }
 *                              When provided, occupied slots become draggable
 *                              and the grid forwards drops via onDrop. Drop
 *                              targets opt out via isDropTarget. Surfaces that
 *                              don't pass dragDrop are unaffected.
 * @returns {HTMLElement} The `.snapshot-grid-wrap` container.
 */
export function buildSnapshotGrid(slots, callbacks = {}) {
    const {
        isSelected = () => false,
        onClick = () => {},
        onDblClick = () => {},
        onKeyActivate,
        makePlayButton = () => null,
        showLegend = true,
        dragDrop = null,
    } = callbacks;

    const wrap = document.createElement('div');
    wrap.className = 'snapshot-grid-wrap';

    const grid = document.createElement('div');
    grid.className = 'snapshot-grid';
    for (const slot of slots) {
        grid.appendChild(buildSlotCell(slot, {
            isSelected, onClick, onDblClick, onKeyActivate, makePlayButton, dragDrop,
        }));
    }
    wrap.appendChild(grid);

    if (showLegend) {
        wrap.appendChild(buildLegend());
    }
    return wrap;
}

function buildSlotCell(slot, cbs) {
    // Use <div role="button"> so nested <button> for play stays valid HTML.
    const cell = document.createElement('div');
    cell.className = 'snapshot-slot';
    cell.setAttribute('role', 'button');
    cell.tabIndex = 0;
    if (slot.empty) cell.classList.add('empty');
    else cell.classList.add('occupied');
    if (slot.changed) cell.classList.add('changed');
    if (slot.duplicate) cell.classList.add('duplicate');
    if (cbs.isSelected(slot)) cell.classList.add('selected');
    cell.dataset.slotKey = slot.slot_key;

    const key = document.createElement('div');
    key.className = 'snapshot-slot-key';
    key.textContent = slot.slot_key;
    cell.appendChild(key);

    const name = document.createElement('div');
    name.className = 'snapshot-slot-name';
    name.textContent = slot.empty ? '-' : (slot.display_name || slot.slot_key);
    cell.appendChild(name);

    if (slot.changed || slot.duplicate) {
        const markers = document.createElement('div');
        markers.className = 'snapshot-slot-markers';
        if (slot.changed) {
            const m = document.createElement('span');
            m.className = 'material-symbols-outlined';
            m.textContent = 'edit';
            m.title = 'Changed vs. compare context';
            markers.appendChild(m);
        }
        if (slot.duplicate) {
            const m = document.createElement('span');
            m.className = 'material-symbols-outlined';
            m.textContent = 'content_copy';
            m.title = 'Duplicate of another slot';
            markers.appendChild(m);
        }
        cell.appendChild(markers);
    }

    if (!slot.empty) {
        const playEl = cbs.makePlayButton(slot);
        if (playEl) {
            const playWrap = document.createElement('div');
            playWrap.className = 'snapshot-slot-play';
            playWrap.appendChild(playEl);
            cell.appendChild(playWrap);
        }
    }

    cell.addEventListener('click', (ev) => cbs.onClick(slot, ev));
    cell.addEventListener('dblclick', (ev) => cbs.onDblClick(slot, ev));
    cell.addEventListener('keydown', (ev) => {
        if (ev.key !== 'Enter' && ev.key !== ' ') return;
        ev.preventDefault();
        const handler = cbs.onKeyActivate || cbs.onClick;
        handler(slot, ev);
    });

    if (cbs.dragDrop) wireDragDrop(cell, slot, cbs.dragDrop);

    return cell;
}

const DRAG_MIME = 'application/x-td3-snapshot-slot';

function wireDragDrop(cell, slot, dragDrop) {
    const {
        isDraggable = (s) => !s.empty,
        isDropTarget = () => true,
        onDrop = () => {},
    } = dragDrop;

    if (isDraggable(slot)) {
        cell.draggable = true;
        cell.addEventListener('dragstart', (ev) => {
            // We move occupied slots only; signal that to the dataTransfer
            // layer so cross-tab drops behave reasonably. The slot_key is
            // both the payload and the identifier.
            ev.dataTransfer.effectAllowed = 'move';
            ev.dataTransfer.setData(DRAG_MIME, slot.slot_key);
            // Plain-text fallback so accidental drops outside the grid show
            // something meaningful instead of "[object Object]".
            ev.dataTransfer.setData('text/plain', slot.slot_key);
            cell.classList.add('dragging');
        });
        cell.addEventListener('dragend', () => {
            cell.classList.remove('dragging');
        });
    }

    // Every cell can be a drop target - the move endpoint accepts both
    // empty (rename) and occupied (swap) destinations. The caller filters
    // via `isDropTarget` (e.g. to forbid dropping onto the source itself).
    cell.addEventListener('dragenter', (ev) => {
        const fromKey = readDragKey(ev);
        if (!fromKey || fromKey === slot.slot_key) return;
        if (!isDropTarget(fromKey, slot)) return;
        ev.preventDefault();
        cell.classList.add('drop-target');
    });
    cell.addEventListener('dragover', (ev) => {
        const fromKey = readDragKey(ev);
        if (!fromKey || fromKey === slot.slot_key) return;
        if (!isDropTarget(fromKey, slot)) return;
        ev.preventDefault();
        ev.dataTransfer.dropEffect = 'move';
    });
    cell.addEventListener('dragleave', () => {
        cell.classList.remove('drop-target');
    });
    cell.addEventListener('drop', (ev) => {
        cell.classList.remove('drop-target');
        const fromKey = readDragKey(ev);
        if (!fromKey || fromKey === slot.slot_key) return;
        if (!isDropTarget(fromKey, slot)) return;
        ev.preventDefault();
        onDrop(fromKey, slot, ev);
    });
}

// dataTransfer.types is a DOMStringList during dragenter/dragover but the
// actual payload is only readable in `drop`. We look at types so we can
// reject early when the drag didn't originate from this grid.
function readDragKey(ev) {
    if (!ev.dataTransfer) return null;
    if (ev.type === 'drop') {
        return ev.dataTransfer.getData(DRAG_MIME) || null;
    }
    const types = ev.dataTransfer.types;
    if (types && Array.from(types).includes(DRAG_MIME)) {
        // A non-empty marker - caller only checks fromKey != slot.slot_key.
        return '__DRAG_IN_PROGRESS__';
    }
    return null;
}

function buildLegend() {
    const legend = document.createElement('div');
    legend.className = 'snapshot-grid-legend';
    legend.appendChild(legendChip('occupied', 'Occupied'));
    legend.appendChild(legendChip('empty',    'Empty'));
    legend.appendChild(legendChip('changed',  'Changed'));
    legend.appendChild(legendChip('duplicate','Duplicate'));
    return legend;
}

function legendChip(className, label) {
    const chip = document.createElement('span');
    chip.className = `snapshot-slot-legend-chip ${className}`;
    chip.textContent = label;
    return chip;
}
