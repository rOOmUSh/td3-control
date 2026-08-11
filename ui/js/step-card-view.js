import { makeCtrlButton, makeAccentPill, makeSlidePill } from './step-card-styles.js';

/** Steps the device plays per quarter note, by playback mode. */
const STEPS_PER_BEAT = 4;
const TRIPLET_STEPS_PER_BEAT = 3;

export function buildStepCardViewModel({
    step,
    index,
    activeSteps,
    selected = false,
    triplet = false,
}) {
    const disabled = index >= activeSteps;
    const isRest = step.time === 'REST' || step.time === 'TIE_REST';
    const isTie = step.time === 'TIE' || step.time === 'TIE_REST';
    // Triplet playback fits three steps into the beat the straight grid
    // gives four, so the downbeats move with the mode: 1/5/9/13 straight,
    // 1/4/7/10 triplet.
    const stepsPerBeat = triplet ? TRIPLET_STEPS_PER_BEAT : STEPS_PER_BEAT;
    const isDownbeat = index % stepsPerBeat === 0;
    const transposeTone = step.transpose === 'UP' ? 'text-lime-400' : 'text-violet-400';
    const showTranspose = step.transpose !== 'NORMAL' && !isRest && !isTie;

    const cardClasses = [
        'step-card',
        'h-16',
        'rounded-lg',
        'flex',
        'flex-col',
        'items-center',
        'justify-center',
        'p-1',
        'relative',
        'overflow-hidden',
        'tactile-card',
    ];

    if (isRest) {
        cardClasses.push('bg-error-container', 'led-glow-red');
    } else if (isTie) {
        cardClasses.push('bg-surface-container', 'border', 'border-outline-variant');
    } else {
        cardClasses.push(
            isDownbeat ? 'bg-surface-container-highest' : 'bg-surface-container-high',
            'hover:bg-surface-container-highest',
        );
    }

    if (isDownbeat) cardClasses.push('step-downbeat');
    if (selected) cardClasses.push('step-kb-selected');

    let noteLabelClass = 'text-base font-black tracking-tighter leading-tight';
    let noteLabelText = step.note;

    if (isRest) {
        noteLabelClass += ' text-on-error-container opacity-70 text-xs';
        noteLabelText = step.time === 'TIE_REST' ? 'T-R' : 'REST';
    } else if (isTie) {
        noteLabelClass += ' text-on-surface-variant opacity-50 text-xs';
        noteLabelText = 'TIE';
    } else {
        noteLabelClass += ' text-on-surface';
    }

    return {
        disabled,
        isRest,
        isTie,
        showTranspose,
        showIndicators: !isRest && !isTie && (step.accent || step.slide),
        cardClassName: cardClasses.join(' '),
        columnClassName: `flex flex-col gap-1 min-w-0 ${disabled ? 'step-disabled' : ''}`.trim(),
        numberClassName: `text-[0.7rem] absolute top-0.5 left-1 font-black ${isRest ? 'text-on-error-container' : 'text-on-surface-variant'}`,
        numberText: String(index + 1).padStart(2, '0'),
        transposeClassName: `text-[0.7rem] absolute top-0.5 right-1 font-black ${transposeTone}`,
        transposeText: step.transpose === 'UP' ? 'UP' : 'DN',
        noteLabelClassName: noteLabelClass,
        noteLabelText,
        controlsClassName: `step-controls grid grid-cols-2 gap-0.5 p-0.5 bg-surface-container rounded-lg ${isRest || isTie ? 'opacity-40' : ''}`.trim(),
    };
}

export function createStepCard({
    step,
    index,
    activeSteps,
    selected = false,
    triplet = false,
    onWheelNoteChange,
    onCardClick,
    onToggleTransposeUp,
    onToggleTransposeDown,
    onToggleSlide,
    onToggleAccent,
}) {
    const view = buildStepCardViewModel({ step, index, activeSteps, selected, triplet });

    const col = document.createElement('div');
    col.className = view.columnClassName;

    const card = document.createElement('div');
    card.className = view.cardClassName;
    card.dataset.step = index;

    const num = document.createElement('span');
    num.className = view.numberClassName;
    num.textContent = view.numberText;
    card.appendChild(num);

    if (view.showTranspose) {
        const tr = document.createElement('span');
        tr.className = view.transposeClassName;
        tr.textContent = view.transposeText;
        card.appendChild(tr);
    }

    const noteLabel = document.createElement('span');
    noteLabel.className = view.noteLabelClassName;
    noteLabel.textContent = view.noteLabelText;
    card.appendChild(noteLabel);

    if (view.showIndicators) {
        const indicators = document.createElement('div');
        indicators.className = 'flex gap-0.5 mt-0.5';
        if (step.accent) indicators.appendChild(makeAccentPill());
        if (step.slide) indicators.appendChild(makeSlidePill());
        card.appendChild(indicators);
    }

    card.addEventListener('wheel', (e) => {
        e.preventDefault();
        if (view.isRest || view.isTie || !onWheelNoteChange) return;
        onWheelNoteChange(e.deltaY < 0 ? 1 : -1);
    });

    card.addEventListener('click', () => {
        if (onCardClick) onCardClick();
    });

    col.appendChild(card);

    const controls = document.createElement('div');
    controls.className = view.controlsClassName;
    // The morph renderer translates the column, so this block travels
    // with its note card. It carries its own source-step index under a
    // distinct attribute that cannot collide with the `data-step`
    // selectors used to find note cards.
    controls.dataset.controlsStep = index;
    controls.appendChild(makeCtrlButton('UP', step.transpose === 'UP', onToggleTransposeUp));
    controls.appendChild(makeCtrlButton('DN', step.transpose === 'DOWN', onToggleTransposeDown));
    controls.appendChild(makeCtrlButton('SL', step.slide, onToggleSlide));
    controls.appendChild(makeCtrlButton('AC', step.accent, onToggleAccent));
    col.appendChild(controls);

    return col;
}
