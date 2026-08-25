// Patched copy of the upstream package: adds the `min`, `max`, `step`,
// `handleWheel`, `handleKeyboard` and `handlePointer` options and exports the
// scene helpers used by `knob-renderer.js`.

import * as THREE from '../vendor/three.module.min.js';
import { GLTFLoader } from '../vendor/loaders/GLTFLoader.js';

export const KNOB_MIN = 0;
export const KNOB_MAX = 127;
export const KNOB_START_DEGREES = -145;
export const KNOB_END_DEGREES = 145;
export const DEFAULT_VALUES = Object.freeze([0, 32, 64, 96, 127]);

export const LIGHT_TARGET = new THREE.Vector3(0, 0.76, 0);
export const DISPLAY_TARGET = new THREE.Vector3(0.01, 0.76, -0.60);
export const DISPLAY_HALF_SIZE = 1.25;
export const DISPLAY_VIEW_CENTER_X = -0.01;
export const DISPLAY_VIEW_CENTER_Y = -0.60;
export const SHADOW_LIFT_MESHES = new Set(['Body', 'Skirt', 'RoofOuterWall']);
export const SHADOW_LIFT_INTENSITY = 0.18;
const modelCache = new Map();

export function midiToDegrees(value) {
  const midi = THREE.MathUtils.clamp(Number(value), KNOB_MIN, KNOB_MAX);
  return THREE.MathUtils.lerp(KNOB_START_DEGREES, KNOB_END_DEGREES, midi / KNOB_MAX);
}

export function midiToRadians(value) {
  return THREE.MathUtils.degToRad(midiToDegrees(value));
}

/** Degrees for `value` over the closed domain `[min, max]`. */
export function valueToDegrees(value, min = KNOB_MIN, max = KNOB_MAX) {
  const span = max - min;
  const clamped = THREE.MathUtils.clamp(Number(value), min, max);
  const ratio = span === 0 ? 0 : (clamped - min) / span;
  return THREE.MathUtils.lerp(KNOB_START_DEGREES, KNOB_END_DEGREES, ratio);
}

/**
 * Quantise `value` to `step` within `[min, max]`. A `step` of 0, null or a
 * non-finite number keeps the full precision of the input.
 */
export function quantizeValue(value, { min = KNOB_MIN, max = KNOB_MAX, step = 1 } = {}) {
  const numeric = Number(value);
  const safe = Number.isFinite(numeric) ? numeric : min;
  const clamped = THREE.MathUtils.clamp(safe, min, max);
  const size = Number(step);
  if (!Number.isFinite(size) || size <= 0) return clamped;
  return THREE.MathUtils.clamp(min + Math.round((clamped - min) / size) * size, min, max);
}

function resolveModelUrl(modelUrl) {
  if (modelUrl) return new URL(modelUrl, document.baseURI).href;
  return new URL('../assets/knob.glb', import.meta.url).href;
}

export function loadModel(modelUrl) {
  const url = resolveModelUrl(modelUrl);
  if (!modelCache.has(url)) {
    const loader = new GLTFLoader();
    modelCache.set(url, loader.loadAsync(url).catch(error => {
      modelCache.delete(url);
      throw error;
    }));
  }
  return modelCache.get(url);
}

function requireCanvas(canvas) {
  if (!(canvas instanceof HTMLCanvasElement)) {
    throw new TypeError('createKnobStrip requires an HTMLCanvasElement in options.canvas');
  }
}

function normalizeValues(values, domain) {
  if (!Array.isArray(values) || values.length === 0) {
    throw new TypeError('options.values must be a non-empty array');
  }
  return values.map(value => quantizeValue(value, domain));
}

export function addWorldFixedStripLights(scene, sourceScene) {
  const sourceSpot = sourceScene.getObjectByName('WorldSpot');
  const sourceOmni = sourceScene.getObjectByName('WorldOmni');
  if (!sourceSpot?.isSpotLight || !sourceOmni?.isPointLight) {
    throw new Error('WorldSpot and WorldOmni punctual lights are missing from knob.glb');
  }

  const spotTarget = new THREE.Object3D();
  spotTarget.position.copy(LIGHT_TARGET);
  const spot = new THREE.SpotLight(
    sourceSpot.color.clone(),
    sourceSpot.intensity,
    0,
    sourceSpot.angle,
    sourceSpot.penumbra,
    sourceSpot.decay
  );
  spot.name = 'KnobStripWorldSpot';
  spot.position.copy(sourceSpot.position);
  spot.target = spotTarget;
  spot.castShadow = true;
  spot.shadow.mapSize.set(1024, 1024);
  spot.shadow.camera.near = 0.2;
  spot.shadow.camera.far = 18;
  spot.shadow.bias = -0.00015;
  spot.shadow.normalBias = 0.012;
  spot.shadow.radius = 9;
  scene.add(spotTarget, spot);

  const omni = new THREE.PointLight(
    sourceOmni.color.clone(),
    sourceOmni.intensity,
    0,
    sourceOmni.decay
  );
  omni.name = 'KnobStripWorldOmni';
  omni.position.copy(sourceOmni.position);
  omni.castShadow = true;
  omni.shadow.mapSize.set(512, 512);
  omni.shadow.camera.near = 0.2;
  omni.shadow.camera.far = 18;
  omni.shadow.bias = -0.0001;
  omni.shadow.normalBias = 0.012;
  omni.shadow.radius = 12;
  scene.add(omni);
}

function emitCanvasEvent(canvas, name, detail) {
  canvas.dispatchEvent(new CustomEvent(name, { detail }));
}

export async function createKnobStrip(options = {}) {
  const {
    canvas,
    modelUrl,
    slotSize = 50,
    values: initialValues = DEFAULT_VALUES,
    supersampling = 4,
    preserveDrawingBuffer = false,
    dragSensitivity = 1.27,
    wheelStep = 4,
    keyboardStep = 1,
    min = KNOB_MIN,
    max = KNOB_MAX,
    step = 1,
    handleWheel = true,
    handleKeyboard = true,
    handlePointer = true,
    onChange,
    onSelect
  } = options;

  requireCanvas(canvas);
  if (!(Number.isFinite(Number(min)) && Number.isFinite(Number(max)) && Number(max) > Number(min))) {
    throw new RangeError('createKnobStrip requires a finite domain with max greater than min');
  }
  const domain = { min: Number(min), max: Number(max), step: Number(step) };
  const toDegrees = value => valueToDegrees(value, domain.min, domain.max);
  const toRadians = value => THREE.MathUtils.degToRad(toDegrees(value));
  // The camera looks down -Y with up set to -Z, so a positive rotation about
  // Y reads counter-clockwise. Negating renders the sweep clockwise-positive.
  const toSceneRotation = value => -toRadians(value);
  const values = normalizeValues(initialValues, domain);
  const cssSlotSize = THREE.MathUtils.clamp(Math.round(Number(slotSize)), 1, 2048);
  const sampleScale = THREE.MathUtils.clamp(Number(supersampling), 1, 4);
  const width = cssSlotSize * values.length;
  const height = cssSlotSize;
  const cleanup = [];
  const roots = [];
  let activeIndex = 0;
  let drag = null;
  let disposed = false;

  canvas.style.width = `${width}px`;
  canvas.style.height = `${height}px`;
  canvas.style.touchAction = 'none';
  if (handleKeyboard) {
    canvas.tabIndex = canvas.tabIndex >= 0 ? canvas.tabIndex : 0;
  }
  canvas.setAttribute('role', 'group');
  canvas.setAttribute('aria-label', `${values.length} interactive GLB knobs, ${cssSlotSize} by ${cssSlotSize} pixels each`);

  const renderer = new THREE.WebGLRenderer({
    canvas,
    alpha: true,
    antialias: true,
    preserveDrawingBuffer: Boolean(preserveDrawingBuffer),
    powerPreference: 'high-performance'
  });
  renderer.setPixelRatio(sampleScale);
  renderer.setSize(width, height, false);
  renderer.setClearColor(0x000000, 0);
  renderer.outputColorSpace = THREE.SRGBColorSpace;
  renderer.toneMapping = THREE.NeutralToneMapping;
  renderer.toneMappingExposure = 1.12;
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.PCFSoftShadowMap;

  const scene = new THREE.Scene();
  const liftedMaterials = new Map();
  const camera = new THREE.OrthographicCamera(
    DISPLAY_VIEW_CENTER_X - DISPLAY_HALF_SIZE,
    DISPLAY_VIEW_CENTER_X + DISPLAY_HALF_SIZE,
    DISPLAY_VIEW_CENTER_Y + DISPLAY_HALF_SIZE,
    DISPLAY_VIEW_CENTER_Y - DISPLAY_HALF_SIZE,
    0.1,
    30
  );
  camera.position.set(0, 10.76, 0);
  camera.up.set(0, 0, -1);
  camera.lookAt(DISPLAY_TARGET);
  camera.userData.target = DISPLAY_TARGET.toArray();

  const gltf = await loadModel(modelUrl);
  const sourceRoot = gltf.scene.getObjectByName('KnobRoot');
  if (!sourceRoot) {
    renderer.dispose();
    throw new Error('KnobRoot is missing from knob.glb');
  }

  addWorldFixedStripLights(scene, gltf.scene);
  values.forEach((value, index) => {
    const root = sourceRoot.clone(true);
    root.name = `Knob_${index}`;
    root.rotation.set(0, toSceneRotation(value), 0);
    root.traverse(child => {
      if (!child.isMesh) return;
      if (SHADOW_LIFT_MESHES.has(child.name)) {
        let liftedMaterial = liftedMaterials.get(child.material.uuid);
        if (!liftedMaterial) {
          liftedMaterial = child.material.clone();
          liftedMaterial.name = `${child.material.name}_ShadowLift`;
          liftedMaterial.emissive.copy(liftedMaterial.color);
          liftedMaterial.emissiveIntensity = SHADOW_LIFT_INTENSITY;
          liftedMaterials.set(child.material.uuid, liftedMaterial);
        }
        child.material = liftedMaterial;
      }
      child.castShadow = true;
      child.receiveShadow = true;
    });
    roots.push(root);
    scene.add(root);
  });

  const render = () => {
    if (disposed) return;
    renderer.setScissorTest(true);
    roots.forEach((activeRoot, index) => {
      roots.forEach(root => { root.visible = root === activeRoot; });
      renderer.setViewport(index * cssSlotSize, 0, cssSlotSize, cssSlotSize);
      renderer.setScissor(index * cssSlotSize, 0, cssSlotSize, cssSlotSize);
      renderer.render(scene, camera);
    });
    roots.forEach(root => { root.visible = true; });
    renderer.setScissorTest(false);
  };

  function detailFor(index, source) {
    return {
      index,
      value: values[index],
      angleDegrees: toDegrees(values[index]),
      angleRadians: toRadians(values[index]),
      source,
      slotSize: cssSlotSize
    };
  }

  function select(index, source = 'api') {
    activeIndex = THREE.MathUtils.clamp(Math.round(index), 0, values.length - 1);
    const detail = detailFor(activeIndex, source);
    onSelect?.(detail);
    emitCanvasEvent(canvas, 'knobselect', detail);
    return detail;
  }

  function setValue(index, nextValue, settings = {}) {
    const targetIndex = THREE.MathUtils.clamp(Math.round(index), 0, values.length - 1);
    const value = quantizeValue(nextValue, domain);
    const changed = values[targetIndex] !== value;
    values[targetIndex] = value;
    roots[targetIndex].rotation.y = toSceneRotation(value);
    if (changed) render();

    const source = settings.source ?? 'api';
    const detail = detailFor(targetIndex, source);
    if (changed && settings.emit !== false) {
      onChange?.(detail);
      emitCanvasEvent(canvas, 'knobchange', detail);
    }
    return value;
  }

  function setValues(nextValues, settings = {}) {
    if (!Array.isArray(nextValues) || nextValues.length !== values.length) {
      throw new RangeError(`setValues expects exactly ${values.length} values`);
    }
    nextValues.forEach((value, index) => setValue(index, value, { ...settings, emit: false }));
    render();
    if (settings.emit !== false) {
      values.forEach((_value, index) => {
        const detail = detailFor(index, settings.source ?? 'api');
        onChange?.(detail);
        emitCanvasEvent(canvas, 'knobchange', detail);
      });
    }
    return [...values];
  }

  function indexFromPointer(event) {
    const rect = canvas.getBoundingClientRect();
    const normalizedX = rect.width > 0 ? (event.clientX - rect.left) / rect.width : 0;
    return THREE.MathUtils.clamp(Math.floor(normalizedX * values.length), 0, values.length - 1);
  }

  function listen(target, type, listener, eventOptions) {
    target.addEventListener(type, listener, eventOptions);
    cleanup.push(() => target.removeEventListener(type, listener, eventOptions));
  }

  // `handlePointer: false` makes the knob a display only and leaves dragging
  // to the host.
  if (handlePointer) {
    listen(canvas, 'pointerdown', event => {
      const index = indexFromPointer(event);
      select(index, 'pointer');
      drag = { index, x: event.clientX, y: event.clientY, value: values[index] };
      canvas.setPointerCapture(event.pointerId);
      canvas.focus({ preventScroll: true });
    });

    listen(canvas, 'pointermove', event => {
      if (!drag) {
        activeIndex = indexFromPointer(event);
        return;
      }
      const deltaPixels = (event.clientX - drag.x) - (event.clientY - drag.y);
      setValue(drag.index, drag.value + deltaPixels * dragSensitivity, { source: 'drag' });
    });

    const endDrag = event => {
      if (canvas.hasPointerCapture(event.pointerId)) canvas.releasePointerCapture(event.pointerId);
      drag = null;
    };
    listen(canvas, 'pointerup', endDrag);
    listen(canvas, 'pointercancel', endDrag);
  }

  // `handleWheel: false` leaves the wheel to the host. The event is not
  // consumed here, so it reaches an ancestor handler once.
  if (handleWheel) {
    listen(canvas, 'wheel', event => {
      event.preventDefault();
      const index = indexFromPointer(event);
      select(index, 'wheel');
      const wheelDelta = event.shiftKey ? wheelStep * 2 : wheelStep;
      setValue(index, values[index] + (event.deltaY < 0 ? wheelDelta : -wheelDelta), { source: 'wheel' });
    }, { passive: false });
  }

  // `handleKeyboard: false` also withholds the tab stop above. The handler
  // calls preventDefault but not stopPropagation, so keys still bubble.
  if (handleKeyboard) {
    listen(canvas, 'keydown', event => {
      let nextValue = null;
      const keyDelta = event.shiftKey ? keyboardStep * 8 : keyboardStep;
      if (event.key === 'ArrowUp' || event.key === 'ArrowRight') nextValue = values[activeIndex] + keyDelta;
      if (event.key === 'ArrowDown' || event.key === 'ArrowLeft') nextValue = values[activeIndex] - keyDelta;
      if (event.key === 'Home') nextValue = domain.min;
      if (event.key === 'End') nextValue = domain.max;
      if (nextValue === null) return;
      event.preventDefault();
      setValue(activeIndex, nextValue, { source: 'keyboard' });
    });
  }

  await renderer.compileAsync(scene, camera);
  render();
  canvas.dataset.knobStripReady = 'true';

  return Object.freeze({
    canvas,
    renderer,
    scene,
    camera,
    roots: Object.freeze(roots),
    gltf,
    slotSize: cssSlotSize,
    width,
    height,
    min: domain.min,
    max: domain.max,
    step: domain.step,
    setValue,
    setValues,
    select,
    getValue: index => values[index],
    getValues: () => [...values],
    render,
    dispose() {
      if (disposed) return;
      disposed = true;
      cleanup.splice(0).forEach(remove => remove());
      liftedMaterials.forEach(material => material.dispose());
      liftedMaterials.clear();
      renderer.dispose();
      canvas.removeAttribute('data-knob-strip-ready');
    }
  });
}
