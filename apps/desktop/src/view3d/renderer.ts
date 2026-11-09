// Draws the scene with WebGL2 (docs/adr/0002-3d-rendering.md).
import { Mat4 } from "./camera";
import { FaceRect } from "./texture";
import { Mesh, Scene } from "./types";

const VERTEX = `#version 300 es
in vec3 aPosition;
in vec3 aNormal;
in vec3 aColor;
uniform mat4 uMatrix;
out vec3 vPosition;
out vec3 vNormal;
out vec3 vColor;
void main() {
  vPosition = aPosition;
  vNormal = aNormal;
  vColor = aColor;
  gl_Position = uMatrix * vec4(aPosition, 1.0);
}`;

const FRAGMENT = `#version 300 es
precision highp float;
in vec3 vPosition;
in vec3 vNormal;
in vec3 vColor;
uniform bool uTextured;
uniform sampler2D uTop;
uniform sampler2D uBottom;
uniform vec4 uRect;
uniform vec3 uEye;
out vec4 color;
void main() {
  vec3 n = normalize(vNormal);
  vec3 base = vColor;
  if (uTextured && abs(n.z) > 0.5) {
    // Scene Y points up, the texture's rows follow the board's Y, pointing down.
    vec2 uv = vec2((vPosition.x - uRect.x) / uRect.z, (-vPosition.y - uRect.y) / uRect.w);
    base = n.z > 0.0 ? texture(uTop, uv).rgb : texture(uBottom, uv).rgb;
  }
  vec3 toEye = normalize(uEye - vPosition);
  // A light at the viewer and a soft one from above, so that no side goes black.
  float head = max(dot(n, toEye), 0.0);
  float sky = 0.5 + 0.5 * n.z;
  vec3 lit = base * (0.32 + 0.52 * head + 0.2 * sky);
  vec3 halfway = normalize(toEye + vec3(0.0, 0.0, 1.0));
  lit += vec3(0.08) * pow(max(dot(n, halfway), 0.0), 32.0);
  color = vec4(lit, 1.0);
}`;

const EDGE_COLOR: [number, number, number] = [0.74, 0.68, 0.46];
const SELECTED: [number, number, number] = [1, 1, 1];
const COLLIDING: [number, number, number] = [0.95, 0.2, 0.15];
const BACKGROUND = [0.11, 0.13, 0.15, 1] as const;

interface Batch {
  vao: WebGLVertexArrayObject;
  buffers: WebGLBuffer[];
  vertices: number;
}

function compile(gl: WebGL2RenderingContext, type: number, source: string): WebGLShader {
  const shader = gl.createShader(type);
  if (!shader) throw new Error("no shader");
  gl.shaderSource(shader, source);
  gl.compileShader(shader);
  if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
    throw new Error(gl.getShaderInfoLog(shader) ?? "shader");
  }
  return shader;
}

export class Renderer {
  private readonly gl: WebGL2RenderingContext;
  private readonly program: WebGLProgram;
  private readonly uniforms: Record<string, WebGLUniformLocation | null>;
  private board: Batch | null = null;
  private parts: Batch | null = null;
  private colors: WebGLBuffer | null = null;
  private partRanges: { id: number; start: number; colors: Float32Array }[] = [];
  private textures: WebGLTexture[] = [];
  private rect: FaceRect | null = null;

  constructor(gl: WebGL2RenderingContext) {
    this.gl = gl;
    const program = gl.createProgram();
    if (!program) throw new Error("no program");
    gl.attachShader(program, compile(gl, gl.VERTEX_SHADER, VERTEX));
    gl.attachShader(program, compile(gl, gl.FRAGMENT_SHADER, FRAGMENT));
    gl.bindAttribLocation(program, 0, "aPosition");
    gl.bindAttribLocation(program, 1, "aNormal");
    gl.bindAttribLocation(program, 2, "aColor");
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(program) ?? "link");
    }
    this.program = program;
    this.uniforms = Object.fromEntries(
      ["uMatrix", "uTextured", "uTop", "uBottom", "uRect", "uEye"].map((name) => [name, gl.getUniformLocation(program, name)]),
    );
    gl.enable(gl.DEPTH_TEST);
    // Models from elsewhere do not always wind their faces outwards, so no culling.
    gl.disable(gl.CULL_FACE);
  }

  private batch(positions: Float32Array, normals: Float32Array, colors: Float32Array): { batch: Batch; colors: WebGLBuffer } {
    const { gl } = this;
    const vao = gl.createVertexArray();
    if (!vao) throw new Error("no vertex array");
    gl.bindVertexArray(vao);
    const buffers = [positions, normals, colors].map((data, location) => {
      const buffer = gl.createBuffer();
      if (!buffer) throw new Error("no buffer");
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferData(gl.ARRAY_BUFFER, data, location === 2 ? gl.DYNAMIC_DRAW : gl.STATIC_DRAW);
      gl.enableVertexAttribArray(location);
      gl.vertexAttribPointer(location, 3, gl.FLOAT, false, 0, 0);
      return buffer;
    });
    gl.bindVertexArray(null);
    return { batch: { vao, buffers, vertices: positions.length / 3 }, colors: buffers[2] as WebGLBuffer };
  }

  private release(batch: Batch | null) {
    if (!batch) return;
    batch.buffers.forEach((buffer) => this.gl.deleteBuffer(buffer));
    this.gl.deleteVertexArray(batch.vao);
  }

  setScene(scene: Scene) {
    this.release(this.board);
    this.release(this.parts);
    this.board = null;
    this.parts = null;
    this.partRanges = [];
    if (scene.board.positions.length > 0) {
      const count = scene.board.positions.length / 3;
      const colors = new Float32Array(count * 3);
      for (let i = 0; i < count; i++) colors.set(EDGE_COLOR, i * 3);
      this.board = this.batch(new Float32Array(scene.board.positions), new Float32Array(scene.board.normals), colors).batch;
    }
    const meshes: { mesh: Mesh; color: [number, number, number]; part: number }[] = [];
    let vertices = 0;
    for (const part of scene.parts) {
      for (const { mesh, color } of part.meshes) {
        meshes.push({ mesh, color, part: part.id });
        vertices += mesh.positions.length / 3;
      }
    }
    if (vertices === 0) return;
    const positions = new Float32Array(vertices * 3);
    const normals = new Float32Array(vertices * 3);
    const colors = new Float32Array(vertices * 3);
    let offset = 0;
    for (const { mesh, color, part } of meshes) {
      positions.set(mesh.positions, offset);
      normals.set(mesh.normals, offset);
      const own = new Float32Array(mesh.positions.length);
      for (let i = 0; i < own.length; i += 3) {
        own[i] = color[0] / 255;
        own[i + 1] = color[1] / 255;
        own[i + 2] = color[2] / 255;
      }
      colors.set(own, offset);
      this.partRanges.push({ id: part, start: offset, colors: own });
      offset += mesh.positions.length;
    }
    const built = this.batch(positions, normals, colors);
    this.parts = built.batch;
    this.colors = built.colors;
  }

  /** Marks parts: colliding ones red, the selected one white. */
  setHighlights(selected: number | null, colliding: ReadonlySet<number>) {
    if (!this.parts || !this.colors) return;
    const { gl } = this;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.colors);
    for (const range of this.partRanges) {
      const mark = range.id === selected ? SELECTED : colliding.has(range.id) ? COLLIDING : null;
      let data = range.colors;
      if (mark) {
        data = new Float32Array(range.colors.length);
        for (let i = 0; i < data.length; i += 3) data.set(mark, i);
      }
      gl.bufferSubData(gl.ARRAY_BUFFER, range.start * 4, data);
    }
  }

  setFaces(rect: FaceRect | null, top: HTMLCanvasElement | null, bottom: HTMLCanvasElement | null) {
    const { gl } = this;
    this.textures.forEach((texture) => gl.deleteTexture(texture));
    this.textures = [];
    this.rect = rect;
    if (!rect || !top || !bottom) return;
    for (const face of [top, bottom]) {
      const texture = gl.createTexture();
      if (!texture) continue;
      gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, face);
      gl.generateMipmap(gl.TEXTURE_2D);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR_MIPMAP_LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      this.textures.push(texture);
    }
  }

  render(matrix: Mat4, eye: [number, number, number], showParts: boolean) {
    const { gl, uniforms } = this;
    gl.viewport(0, 0, gl.drawingBufferWidth, gl.drawingBufferHeight);
    gl.clearColor(...BACKGROUND);
    gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    gl.useProgram(this.program);
    gl.uniformMatrix4fv(uniforms.uMatrix as WebGLUniformLocation, false, matrix);
    gl.uniform3f(uniforms.uEye as WebGLUniformLocation, ...eye);
    gl.uniform1i(uniforms.uTop as WebGLUniformLocation, 0);
    gl.uniform1i(uniforms.uBottom as WebGLUniformLocation, 1);
    if (this.board) {
      const textured = this.textures.length === 2 && this.rect !== null;
      gl.uniform1i(uniforms.uTextured as WebGLUniformLocation, textured ? 1 : 0);
      if (textured && this.rect) {
        gl.uniform4f(uniforms.uRect as WebGLUniformLocation, this.rect.minX, this.rect.minY, this.rect.width, this.rect.height);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, this.textures[0] as WebGLTexture);
        gl.activeTexture(gl.TEXTURE1);
        gl.bindTexture(gl.TEXTURE_2D, this.textures[1] as WebGLTexture);
      }
      gl.bindVertexArray(this.board.vao);
      gl.drawArrays(gl.TRIANGLES, 0, this.board.vertices);
    }
    if (showParts && this.parts) {
      gl.uniform1i(uniforms.uTextured as WebGLUniformLocation, 0);
      gl.bindVertexArray(this.parts.vao);
      gl.drawArrays(gl.TRIANGLES, 0, this.parts.vertices);
    }
    gl.bindVertexArray(null);
  }
}
