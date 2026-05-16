import { makeCtrlButton, makeAccentPill, makeSlidePill } from './step-card-styles.js';

export function buildStepCardViewModel({ step, index, activeSteps, selected = false }) {
    const disabled = index >= activeSteps;
    const isRest = step.time === 'REST' || step.time === 'TIE_REST';
    const isTie = step.time === 'TIE' || step.time === 'TIE_REST';
    const isDownbeat = index % 4 === 0;
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
            'transition-all',
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
        controlsClassName: `grid grid-cols-2 gap-0.5 p-0.5 bg-surface-container rounded-lg ${isRest || isTie ? 'opacity-40' : ''}`.trim(),
    };
}

export function createStepCard({
    step,
    index,
    activeSteps,
    selected = false,
    onWheelNoteChange,
    onCardClick,
    onToggleTransposeUp,
    onToggleTransposeDown,
    onToggleSlide,
    onToggleAccent,
}) {
    const view = buildStepCardViewModel({ step, index, activeSteps, selected });

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
    controls.appendChild(makeCtrlButton('UP', step.transpose === 'UP', onToggleTransposeUp));
    controls.appendChild(makeCtrlButton('DN', step.transpose === 'DOWN', onToggleTransposeDown));
    controls.appendChild(makeCtrlButton('SL', step.slide, onToggleSlide));
    controls.appendChild(makeCtrlButton('AC', step.accent, onToggleAccent));
    col.appendChild(controls);

    return col;
}
