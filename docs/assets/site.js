// The hero: Hadea as a contour map. A terrain mesh drawn only as topographic lines
// (the game's own motif) in fog, with a route traced across it in Lymbic blue — the
// minimap's job, shown rather than told. Everything else on the page is static HTML.
import * as THREE from 'three';

const canvas = document.getElementById('terrain');
const still = matchMedia('(prefers-reduced-motion: reduce)').matches;

// value noise, a few octaves: hills, not spikes
function noise2(seed) {
  const p = new Uint8Array(512);
  let s = seed;
  for (let i = 0; i < 256; i++) p[i] = i;
  for (let i = 255; i > 0; i--) { s = (s * 16807) % 2147483647; const j = s % (i + 1); [p[i], p[j]] = [p[j], p[i]]; }
  for (let i = 0; i < 256; i++) p[i + 256] = p[i];
  const g = (h) => (p[h] / 255) * 2 - 1;
  const f = (t) => t * t * (3 - 2 * t);
  return (x, y) => {
    const xi = Math.floor(x) & 255, yi = Math.floor(y) & 255, xf = x - Math.floor(x), yf = y - Math.floor(y);
    const a = g(p[xi] + yi), b = g(p[xi + 1] + yi), c = g(p[xi] + yi + 1), d = g(p[xi + 1] + yi + 1);
    const u = f(xf), v = f(yf);
    return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
  };
}
const n = noise2(1620730);
const height = (x, z) => {
  let h = 0, amp = 1, fr = 0.035;
  for (let o = 0; o < 5; o++) { h += n(x * fr, z * fr) * amp; amp *= 0.5; fr *= 2.1; }
  // a valley down the middle where the route runs
  const valley = Math.exp(-((x - Math.sin(z * 0.05) * 14) ** 2) / 220);
  return h * 13 - valley * 7;
};

function start() {
  let renderer;
  try {
    // MSAA only touches the full-resolution passes (the upscale, the depth fill, the
    // route), which are cheap; the terrain's contours antialias themselves (fwidth).
    renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: false, powerPreference: 'low-power', precision: 'mediump' });
  } catch {
    document.documentElement.classList.add('no-webgl');
    return;
  }
  // Two resolutions. The terrain — its contour shader is nearly all the cost — is
  // drawn into a buffer at about two thirds of the CSS pixels and scaled up
  // smoothly; fog and soft lines lose nothing visible. The route and the beacon,
  // thin and sharp, are drawn over it at the screen's own resolution, against the
  // terrain's depth filled in again at full size (depth only: no shading), so the
  // hills still hide them. The terrain's scale adapts (see frame) from 0.45 to 0.85.
  const FULL = Math.min(devicePixelRatio, 1.5);
  const MAX_SCALE = 0.85, MIN_SCALE = 0.45;
  let scale = 0.66;
  renderer.setPixelRatio(FULL);
  renderer.autoClear = false;
  const low = new THREE.WebGLRenderTarget(1, 1, { minFilter: THREE.LinearFilter, magFilter: THREE.LinearFilter, depthBuffer: true });
  const scene = new THREE.Scene();
  const overlay = new THREE.Scene();
  const ground = new THREE.Color('#0e1217');
  scene.background = ground;
  scene.fog = new THREE.FogExp2(ground, 0.0105);
  const camera = new THREE.PerspectiveCamera(48, 1, 1, 230); // past ~200 m it is all fog

  // the terrain: contour lines in the fragment shader, every 1.6 m, every fifth heavier
  const size = 260, seg = 160; // ~51k triangles: fewer and the contour lines kink at the quads
  const geo = new THREE.PlaneGeometry(size, size, seg, seg);
  geo.rotateX(-Math.PI / 2);
  const pos = geo.attributes.position;
  for (let i = 0; i < pos.count; i++) pos.setY(i, height(pos.getX(i), pos.getZ(i)));
  geo.computeVertexNormals();
  const uniforms = {
    uTime: { value: 0 },
    uFog: { value: ground },
    uLine: { value: new THREE.Color('#8b96a3') },
    uAccent: { value: new THREE.Color('#5a9ce6') },
    uFocus: { value: new THREE.Vector2(0, 0) },
    uFogDensity: { value: 0.0105 },
  };
  const mat = new THREE.ShaderMaterial({
    uniforms,
    extensions: { derivatives: true },
    vertexShader: /* glsl */ `
      varying float vH; varying vec3 vW; varying float vDepth; varying vec3 vN;
      void main() {
        vec4 w = modelMatrix * vec4(position, 1.0);
        vW = w.xyz; vH = position.y; vN = normal;
        vec4 mv = viewMatrix * w; vDepth = -mv.z;
        gl_Position = projectionMatrix * mv;
      }`,
    fragmentShader: /* glsl */ `
      uniform float uTime; uniform vec3 uFog; uniform vec3 uLine; uniform vec3 uAccent;
      uniform vec2 uFocus; uniform float uFogDensity;
      varying float vH; varying vec3 vW; varying float vDepth; varying vec3 vN;
      float contour(float v, float step, float width) {
        float f = abs(fract(v / step - 0.5) - 0.5) / fwidth(v / step);
        return 1.0 - clamp(f / width, 0.0, 1.0);
      }
      void main() {
        float minor = contour(vH, 1.6, 1.0);
        float major = contour(vH, 8.0, 1.4);
        float shade = 0.5 + 0.5 * dot(normalize(vN), normalize(vec3(-0.5, 0.8, 0.3)));
        vec3 base = mix(uFog, vec3(0.085, 0.105, 0.13), shade * 0.9);
        vec3 col = base + uLine * (minor * 0.22 + major * 0.55);
        // a slow sweep of light rippling out from the focus point
        float d = length(vW.xz - uFocus);
        float ring = smoothstep(2.5, 0.0, abs(d - mod(uTime * 9.0, 140.0))) * smoothstep(140.0, 30.0, d);
        col += uAccent * ring * (minor + major) * 0.9;
        float fog = 1.0 - exp(-uFogDensity * uFogDensity * vDepth * vDepth);
        gl_FragColor = vec4(mix(col, uFog, clamp(fog, 0.0, 1.0)), 1.0);
      }`,
  });
  scene.add(new THREE.Mesh(geo, mat));
  const depthOnly = new THREE.Mesh(geo, new THREE.MeshBasicMaterial({ colorWrite: false }));
  depthOnly.renderOrder = -1;
  overlay.add(depthOnly);

  // the low-resolution terrain, stretched over the screen
  const flat = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  const upscale = new THREE.Scene();
  upscale.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), new THREE.ShaderMaterial({
    uniforms: { uLow: { value: low.texture } },
    depthTest: false,
    depthWrite: false,
    vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy, 0.0, 1.0); }`,
    fragmentShader: `uniform sampler2D uLow; varying vec2 vUv; void main(){ gl_FragColor = texture2D(uLow, vUv); }`,
  })));

  // the route: along the valley floor, a little above it
  const pts = [];
  for (let z = 90; z >= -60; z -= 2) {
    const x = Math.sin(z * 0.05) * 14 + Math.sin(z * 0.17) * 2.5;
    pts.push(new THREE.Vector3(x, height(x, z) + 0.6, z));
  }
  const curve = new THREE.CatmullRomCurve3(pts);
  const tube = new THREE.TubeGeometry(curve, 220, 0.22, 4, false);
  const routeMat = new THREE.ShaderMaterial({
    uniforms: { uTime: uniforms.uTime, uAccent: uniforms.uAccent },
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
    vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.0); }`,
    fragmentShader: /* glsl */ `
      uniform float uTime; uniform vec3 uAccent; varying vec2 vUv;
      void main() {
        float grow = smoothstep(0.0, 1.0, clamp(uTime * 0.18, 0.0, 1.0));
        if (vUv.x > grow) discard;
        float dash = step(0.45, fract(vUv.x * 90.0 - uTime * 1.2));
        float tip = smoothstep(grow - 0.04, grow, vUv.x);
        gl_FragColor = vec4(uAccent * (0.55 + 0.45 * dash) + tip * 0.6, 0.85);
      }`,
  });
  overlay.add(new THREE.Mesh(tube, routeMat));

  // the goal: a beacon at the route's end
  const end = curve.getPoint(1);
  const beacon = new THREE.Mesh(
    new THREE.CylinderGeometry(0.05, 0.6, 14, 16, 1, true),
    new THREE.MeshBasicMaterial({ color: '#5a9ce6', transparent: true, opacity: 0.25, blending: THREE.AdditiveBlending, depthWrite: false, side: THREE.DoubleSide }),
  );
  beacon.position.set(end.x, end.y + 7, end.z);
  overlay.add(beacon);
  uniforms.uFocus.value.set(end.x, end.z);

  // camera: a slow drift down the valley, eased toward the pointer
  const pointer = { x: 0, y: 0, tx: 0, ty: 0 };
  addEventListener('pointermove', (e) => {
    pointer.tx = (e.clientX / innerWidth) * 2 - 1;
    pointer.ty = (e.clientY / innerHeight) * 2 - 1;
  }, { passive: true });
  let scroll = 0;
  addEventListener('scroll', () => { scroll = Math.min(scrollY / innerHeight, 1); }, { passive: true });

  function resize() {
    const w = canvas.clientWidth, h = canvas.clientHeight;
    renderer.setSize(w, h, false);
    low.setSize(Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale)));
    camera.aspect = w / h;
    camera.fov = w < 700 ? 62 : 48;
    camera.updateProjectionMatrix();
  }
  new ResizeObserver(resize).observe(canvas);
  resize();

  let visible = true;
  new IntersectionObserver(([e]) => { visible = e.isIntersecting; }).observe(canvas);

  const clock = new THREE.Clock();
  const look = new THREE.Vector3();
  function frame() {
    const t = still ? 6 : clock.getElapsedTime();
    uniforms.uTime.value = t;
    pointer.x += (pointer.tx - pointer.x) * 0.04;
    pointer.y += (pointer.ty - pointer.y) * 0.04;
    const z = 95 - (t * 1.4) % 60;
    camera.position.set(Math.sin(t * 0.05) * 10 + pointer.x * 6, 26 - pointer.y * 3 + scroll * 18, z);
    look.set(Math.sin((z - 40) * 0.05) * 10, 0, z - 48);
    camera.lookAt(look);
    beacon.material.opacity = 0.18 + 0.1 * Math.sin(t * 2.0);
    renderer.setRenderTarget(low);
    renderer.clear();
    renderer.render(scene, camera);
    renderer.setRenderTarget(null);
    renderer.clear();
    renderer.render(upscale, flat);
    renderer.render(overlay, camera);
  }
  if (still) {
    frame();
    new ResizeObserver(frame).observe(canvas);
  } else {
    // 30 fps is plenty for a slow drift, and half the work of 60. The interval
    // between drawn frames also measures the GPU: a run of slow ones lowers the
    // resolution, a run of quick ones raises it back.
    const STEP = 1000 / 30;
    let last = 0, slow = 0, quick = 0;
    renderer.setAnimationLoop((now) => {
      if (!visible || document.hidden || now - last < STEP - 2) return;
      const dt = last ? now - last : STEP;
      last = now;
      frame();
      if (dt > STEP * 1.5) { slow++; quick = 0; } else if (dt < STEP * 1.1) { quick++; slow = 0; }
      if (slow > 20 && scale > MIN_SCALE) { scale = Math.max(MIN_SCALE, scale - 0.15); resize(); slow = 0; }
      if (quick > 240 && scale < MAX_SCALE) { scale = Math.min(MAX_SCALE, scale + 0.1); resize(); quick = 0; }
    });
  }
  document.documentElement.classList.add('webgl');
}

// the hero's compass strip drifts with the pointer: the HUD the game doesn't have
const track = document.querySelector('.compass-track');
if (track && !still) {
  addEventListener('pointermove', (e) => {
    track.style.setProperty('--turn', `${((e.clientX / innerWidth) - 0.5) * -120}px`);
  }, { passive: true });
}

// the spoiler demo
for (const v of document.querySelectorAll('.vault')) {
  const b = v.querySelector('.reveal');
  b.addEventListener('click', () => {
    const hidden = v.dataset.hidden === 'true';
    v.dataset.hidden = hidden ? 'false' : 'true';
    b.textContent = hidden ? b.dataset.hide : b.dataset.show;
    b.setAttribute('aria-pressed', String(hidden));
  });
}

// the film plays while it is on screen (it is muted); with reduced motion it waits
// for the player's own controls
for (const v of document.querySelectorAll('.film video')) {
  if (still) { v.controls = true; v.preload = 'metadata'; continue; }
  new IntersectionObserver(([e]) => { if (e.isIntersecting) v.play().catch(() => { v.controls = true; }); else v.pause(); },
    { threshold: 0.35 }).observe(v);
}

// sections fade up once as they arrive
const io = new IntersectionObserver((es) => {
  for (const e of es) if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
}, { rootMargin: '0px 0px -10% 0px' });
for (const s of document.querySelectorAll('main > section:not(.hero)')) io.observe(s);

// the language menu closes on outside click
const langs = document.querySelector('.langs');
document.addEventListener('click', (e) => { if (langs && !langs.contains(e.target)) langs.open = false; });

// Start after the page has painted and the browser is idle, so the 3D scene never
// delays the text; a phone that asks to save data gets the CSS contours instead.
const saveData = navigator.connection && navigator.connection.saveData;
if (!saveData) {
  const go = () => start();
  if ('requestIdleCallback' in window) requestIdleCallback(go, { timeout: 1500 });
  else setTimeout(go, 200);
}
