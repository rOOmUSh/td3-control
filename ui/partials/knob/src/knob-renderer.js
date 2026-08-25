// One shared WebGL context that paints the knob into ordinary 2D canvases.
//
// The knob is rendered offscreen and blitted with `drawImage`, so a page can
// show any number of faces while holding a single context. The scene,
// lighting, camera and materials are imported from `knob-strip.js`.

import * as THREE from '../vendor/three.module.min.js';
import {
    DISPLAY_HALF_SIZE,
    DISPLAY_TARGET,
    DISPLAY_VIEW_CENTER_X,
    DISPLAY_VIEW_CENTER_Y,
    KNOB_END_DEGREES,
    KNOB_START_DEGREES,
    SHADOW_LIFT_INTENSITY,
    SHADOW_LIFT_MESHES,
    addWorldFixedStripLights,
    loadModel,
} from './knob-strip.js';

/** Offscreen render square. Every knob blits down from this. */
const RENDER_PIXELS = 256;

let shared = null;

/**
 * Degrees for `value` over `[min, max]`, clockwise-positive from the top.
 * The sweep runs from -145 at the minimum to +145 at the maximum.
 */
export function knobDegrees(value, min, max) {
    const span = max - min;
    const clamped = THREE.MathUtils.clamp(Number(value), min, max);
    const ratio = span === 0 ? 0 : (clamped - min) / span;
    return THREE.MathUtils.lerp(KNOB_START_DEGREES, KNOB_END_DEGREES, ratio);
}

function applyShadowLift(root, liftedMaterials) {
    root.traverse((child) => {
        if (!child.isMesh) return;
        if (SHADOW_LIFT_MESHES.has(child.name)) {
            let lifted = liftedMaterials.get(child.material.uuid);
            if (!lifted) {
                lifted = child.material.clone();
                lifted.name = `${child.material.name}_ShadowLift`;
                lifted.emissive.copy(lifted.color);
                lifted.emissiveIntensity = SHADOW_LIFT_INTENSITY;
                liftedMaterials.set(child.material.uuid, lifted);
            }
            child.material = lifted;
        }
        child.castShadow = true;
        child.receiveShadow = true;
    });
}

async function build(modelUrl) {
    const canvas = document.createElement('canvas');
    canvas.width = RENDER_PIXELS;
    canvas.height = RENDER_PIXELS;

    const renderer = new THREE.WebGLRenderer({
        canvas,
        alpha: true,
        antialias: true,
        powerPreference: 'high-performance',
        // The blit reads the buffer after `render` returns.
        preserveDrawingBuffer: true,
    });
    renderer.setPixelRatio(1);
    renderer.setSize(RENDER_PIXELS, RENDER_PIXELS, false);
    renderer.setClearColor(0x000000, 0);
    renderer.outputColorSpace = THREE.SRGBColorSpace;
    renderer.toneMapping = THREE.NeutralToneMapping;
    renderer.toneMappingExposure = 1.12;
    renderer.shadowMap.enabled = true;
    renderer.shadowMap.type = THREE.PCFSoftShadowMap;

    const gltf = await loadModel(modelUrl);
    const sourceRoot = gltf.scene.getObjectByName('KnobRoot');
    if (!sourceRoot) {
        renderer.dispose();
        throw new Error('KnobRoot is missing from knob.glb');
    }

    const scene = new THREE.Scene();
    addWorldFixedStripLights(scene, gltf.scene);

    const liftedMaterials = new Map();
    const root = sourceRoot.clone(true);
    root.name = 'KnobShared';
    applyShadowLift(root, liftedMaterials);
    scene.add(root);

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

    let disposed = false;

    return {
        renderPixels: RENDER_PIXELS,
        /**
         * Draw the knob at `degrees` into `target`, a 2D canvas. Returns
         * false when the renderer is disposed or `target` has no 2D context.
         */
        paint(target, degrees) {
            if (disposed || !target) return false;
            const context = target.getContext('2d');
            if (!context) return false;
            // KnobRoot carries a baked rotation on all three axes, so `set`
            // is required: assigning `rotation.y` alone renders it flipped.
            root.rotation.set(0, -THREE.MathUtils.degToRad(degrees), 0);
            renderer.render(scene, camera);
            context.clearRect(0, 0, target.width, target.height);
            context.drawImage(canvas, 0, 0, target.width, target.height);
            return true;
        },
        dispose() {
            if (disposed) return;
            disposed = true;
            liftedMaterials.forEach((material) => material.dispose());
            liftedMaterials.clear();
            renderer.dispose();
            shared = null;
        },
    };
}

/** The page-wide renderer, built on first use and shared by every caller. */
export function getKnobRenderer({ modelUrl } = {}) {
    if (!shared) {
        shared = build(modelUrl).catch((error) => {
            shared = null;
            throw error;
        });
    }
    return shared;
}
