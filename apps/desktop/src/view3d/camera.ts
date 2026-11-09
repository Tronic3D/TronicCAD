// Camera and matrix arithmetic of the 3D view. Pure functions, tested separately.
// Matrices are column-major, as WebGL wants them. Z points out of the board's top.

export type Vec3 = [number, number, number];
export type Mat4 = Float32Array;

export interface Camera {
  target: Vec3;
  distance: number;
  /** Around Z, in degrees. -90 looks along +Y. */
  azimuth: number;
  /** Above the board, in degrees. */
  elevation: number;
  perspective: boolean;
}

export const FIELD_OF_VIEW = 35;
const MAX_ELEVATION = 89.9;

export const PRESETS = {
  top: { azimuth: -90, elevation: MAX_ELEVATION },
  bottom: { azimuth: -90, elevation: -MAX_ELEVATION },
  front: { azimuth: -90, elevation: 0 },
  side: { azimuth: 0, elevation: 0 },
  iso: { azimuth: -60, elevation: 35 },
} as const;

export type Preset = keyof typeof PRESETS;

const sub = (a: Vec3, b: Vec3): Vec3 => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a: Vec3, b: Vec3): Vec3 => [
  a[1] * b[2] - a[2] * b[1],
  a[2] * b[0] - a[0] * b[2],
  a[0] * b[1] - a[1] * b[0],
];
const dot = (a: Vec3, b: Vec3) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
function normalize(a: Vec3): Vec3 {
  const length = Math.hypot(a[0], a[1], a[2]) || 1;
  return [a[0] / length, a[1] / length, a[2] / length];
}

export function eyePosition(camera: Camera): Vec3 {
  const az = (camera.azimuth * Math.PI) / 180;
  const el = (camera.elevation * Math.PI) / 180;
  return [
    camera.target[0] + camera.distance * Math.cos(el) * Math.cos(az),
    camera.target[1] + camera.distance * Math.cos(el) * Math.sin(az),
    camera.target[2] + camera.distance * Math.sin(el),
  ];
}

export function multiply(a: Mat4, b: Mat4): Mat4 {
  const out = new Float32Array(16);
  for (let column = 0; column < 4; column++) {
    for (let row = 0; row < 4; row++) {
      let sum = 0;
      for (let k = 0; k < 4; k++) sum += (a[k * 4 + row] as number) * (b[column * 4 + k] as number);
      out[column * 4 + row] = sum;
    }
  }
  return out;
}

function lookAt(eye: Vec3, target: Vec3, up: Vec3): Mat4 {
  const f = normalize(sub(target, eye));
  const s = normalize(cross(f, up));
  const u = cross(s, f);
  return new Float32Array([
    s[0], u[0], -f[0], 0,
    s[1], u[1], -f[1], 0,
    s[2], u[2], -f[2], 0,
    -dot(s, eye), -dot(u, eye), dot(f, eye), 1,
  ]);
}

function perspective(fovDegrees: number, aspect: number, near: number, far: number): Mat4 {
  const f = 1 / Math.tan((fovDegrees * Math.PI) / 360);
  return new Float32Array([
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, (far + near) / (near - far), -1,
    0, 0, (2 * far * near) / (near - far), 0,
  ]);
}

function orthographic(halfHeight: number, aspect: number, near: number, far: number): Mat4 {
  return new Float32Array([
    1 / (halfHeight * aspect), 0, 0, 0,
    0, 1 / halfHeight, 0, 0,
    0, 0, -2 / (far - near), 0,
    0, 0, -(far + near) / (far - near), 1,
  ]);
}

/** Half the height the view shows at the target. Both projections agree on it. */
export const halfHeightAt = (camera: Camera) => camera.distance * Math.tan((FIELD_OF_VIEW * Math.PI) / 360);

/** World to clip space. `radius` bounds the scene, for the depth range. */
export function viewProjection(camera: Camera, aspect: number, radius: number): Mat4 {
  const eye = eyePosition(camera);
  const view = lookAt(eye, camera.target, [0, 0, 1]);
  const far = camera.distance + radius * 2 + 1;
  const near = Math.max(camera.distance - radius * 2, camera.distance * 0.01, 0.01);
  const projection = camera.perspective
    ? perspective(FIELD_OF_VIEW, aspect, near, far)
    : orthographic(halfHeightAt(camera), aspect, near, far);
  return multiply(projection, view);
}

/** Where a world point appears, in the range -1 to 1 on both axes. */
export function project(matrix: Mat4, point: Vec3): [number, number] {
  const [x, y, z] = point;
  const m = matrix as unknown as number[];
  const w = (m[3] as number) * x + (m[7] as number) * y + (m[11] as number) * z + (m[15] as number);
  return [
    ((m[0] as number) * x + (m[4] as number) * y + (m[8] as number) * z + (m[12] as number)) / w,
    ((m[1] as number) * x + (m[5] as number) * y + (m[9] as number) * z + (m[13] as number)) / w,
  ];
}

export function rotated(camera: Camera, dx: number, dy: number): Camera {
  return {
    ...camera,
    azimuth: ((((camera.azimuth - dx * 0.4 + 180) % 360) + 360) % 360) - 180,
    elevation: Math.max(-MAX_ELEVATION, Math.min(MAX_ELEVATION, camera.elevation + dy * 0.4)),
  };
}

/** Moves the view by pixels on a canvas of the given height. */
export function panned(camera: Camera, dx: number, dy: number, height: number): Camera {
  const eye = eyePosition(camera);
  const f = normalize(sub(camera.target, eye));
  const s = normalize(cross(f, [0, 0, 1]));
  const u = cross(s, f);
  const perPixel = (2 * halfHeightAt(camera)) / Math.max(height, 1);
  return {
    ...camera,
    target: [
      camera.target[0] - s[0] * dx * perPixel + u[0] * dy * perPixel,
      camera.target[1] - s[1] * dx * perPixel + u[1] * dy * perPixel,
      camera.target[2] - s[2] * dx * perPixel + u[2] * dy * perPixel,
    ],
  };
}

export function zoomed(camera: Camera, factor: number, radius: number): Camera {
  return { ...camera, distance: Math.max(radius * 0.02, Math.min(radius * 40, camera.distance * factor)) };
}

export interface Bounds {
  min: Vec3;
  max: Vec3;
}

export function boundsOf(lists: (number[] | Float32Array)[]): Bounds {
  const min: Vec3 = [Infinity, Infinity, Infinity];
  const max: Vec3 = [-Infinity, -Infinity, -Infinity];
  for (const positions of lists) {
    for (let i = 0; i + 2 < positions.length; i += 3) {
      for (let axis = 0; axis < 3; axis++) {
        const value = positions[i + axis] as number;
        if (value < (min[axis] as number)) min[axis] = value;
        if (value > (max[axis] as number)) max[axis] = value;
      }
    }
  }
  if (!Number.isFinite(min[0])) return { min: [-10, -10, -1], max: [10, 10, 1] };
  return { min, max };
}

export const radiusOf = (bounds: Bounds) =>
  Math.max(Math.hypot(bounds.max[0] - bounds.min[0], bounds.max[1] - bounds.min[1], bounds.max[2] - bounds.min[2]) / 2, 1);

/** A camera showing the whole scene from the given direction. */
export function fitted(bounds: Bounds, aspect: number, preset: Preset, isPerspective: boolean): Camera {
  const radius = radiusOf(bounds);
  const tangent = Math.tan((FIELD_OF_VIEW * Math.PI) / 360);
  const fit = aspect >= 1 ? tangent : tangent * aspect;
  return {
    target: [
      (bounds.min[0] + bounds.max[0]) / 2,
      (bounds.min[1] + bounds.max[1]) / 2,
      (bounds.min[2] + bounds.max[2]) / 2,
    ],
    distance: (radius / fit) * 1.1,
    ...PRESETS[preset],
    perspective: isPerspective,
  };
}
