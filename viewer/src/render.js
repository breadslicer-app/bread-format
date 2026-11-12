// SPDX-FileCopyrightText: 2026 SynrgStudio
// SPDX-License-Identifier: Apache-2.0

// A small WebGL2 renderer for the viewer: meshes placed by world matrices,
// lit with flat shading worked out in the fragment shader, so a mesh of
// millions of triangles needs no normals and no copy of its vertices; a
// millimetre grid as the build plate, Z up; and an orbit camera driven by
// mouse, wheel and touch.

const MESH_VS = `#version 300 es
in vec3 position;
uniform mat4 model;
uniform mat4 viewProjection;
out vec3 world;
void main() {
	vec4 p = model * vec4(position, 1.0);
	world = p.xyz;
	gl_Position = viewProjection * p;
}`;

const MESH_FS = `#version 300 es
precision highp float;
in vec3 world;
uniform vec3 color;
uniform vec3 eye;
out vec4 outColor;
void main() {
	// The face's normal, from how the position changes across the pixel.
	vec3 n = normalize(cross(dFdx(world), dFdy(world)));
	vec3 toEye = normalize(eye - world);
	// Lit from the eye and from above, both sides alike, so a mesh with
	// inverted faces still shows.
	float head = abs(dot(n, toEye));
	float top = abs(dot(n, normalize(vec3(0.3, 0.2, 1.0))));
	float light = 0.22 + 0.58 * head + 0.25 * top;
	outColor = vec4(color * light, 1.0);
}`;

const LINE_VS = `#version 300 es
in vec3 position;
in vec3 lineColor;
uniform mat4 viewProjection;
out vec3 vColor;
void main() {
	vColor = lineColor;
	gl_Position = viewProjection * vec4(position, 1.0);
}`;

const LINE_FS = `#version 300 es
precision mediump float;
in vec3 vColor;
out vec4 outColor;
void main() {
	outColor = vec4(vColor, 1.0);
}`;

/** Colours, as Breadslicer draws them. */
export const COLORS = {
	ground: [0x11 / 255, 0x13 / 255, 0x15 / 255],
	model: [0.82, 0.82, 0.8],
	support: [0.38, 0.44, 0.53],
	selected: [0xe2 / 255, 0xa7 / 255, 0x50 / 255],
	gridMinor: [0x1c / 255, 0x20 / 255, 0x24 / 255],
	gridMajor: [0x2c / 255, 0x33 / 255, 0x39 / 255],
	axisX: [0.75, 0.32, 0.32],
	axisY: [0.36, 0.68, 0.45],
};

const GRID_MM = 10;
const GRID_MAJOR_EVERY = 5;

function compile(gl, vs, fs) {
	const program = gl.createProgram();
	for (const [type, source] of [
		[gl.VERTEX_SHADER, vs],
		[gl.FRAGMENT_SHADER, fs],
	]) {
		const shader = gl.createShader(type);
		gl.shaderSource(shader, source);
		gl.compileShader(shader);
		if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
			throw new Error(gl.getShaderInfoLog(shader) ?? 'shader');
		}
		gl.attachShader(program, shader);
	}
	gl.linkProgram(program);
	if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
		throw new Error(gl.getProgramInfoLog(program) ?? 'program');
	}
	return program;
}

// --- 4 × 4 matrices, column-major, in f64 ---------------------------------

function multiply(a, b) {
	const out = new Float64Array(16);
	for (let c = 0; c < 4; c++) {
		for (let r = 0; r < 4; r++) {
			out[4 * c + r] =
				a[r] * b[4 * c] + a[4 + r] * b[4 * c + 1] + a[8 + r] * b[4 * c + 2] + a[12 + r] * b[4 * c + 3];
		}
	}
	return out;
}

function perspective(fovY, aspect, near, far) {
	const f = 1 / Math.tan(fovY / 2);
	const out = new Float64Array(16);
	out[0] = f / aspect;
	out[5] = f;
	out[10] = (far + near) / (near - far);
	out[11] = -1;
	out[14] = (2 * far * near) / (near - far);
	return out;
}

function lookAt(eye, target, up) {
	const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
	const norm = (v) => {
		const l = Math.hypot(v[0], v[1], v[2]) || 1;
		return [v[0] / l, v[1] / l, v[2] / l];
	};
	const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
	const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
	const z = norm(sub(eye, target));
	const x = norm(cross(up, z));
	const y = cross(z, x);
	return Float64Array.of(
		x[0], y[0], z[0], 0,
		x[1], y[1], z[1], 0,
		x[2], y[2], z[2], 0,
		-dot(x, eye), -dot(y, eye), -dot(z, eye), 1,
	);
}

/** A point through a column-major matrix. */
export function transformPoint(m, p) {
	return [
		m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
		m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
		m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
	];
}

/** The box around a mesh's vertices, skipping coordinates that are not finite. */
export function meshBounds(vertices) {
	const min = [Infinity, Infinity, Infinity];
	const max = [-Infinity, -Infinity, -Infinity];
	for (let i = 0; i < vertices.length; i += 3) {
		const x = vertices[i];
		const y = vertices[i + 1];
		const z = vertices[i + 2];
		if (!Number.isFinite(x) || !Number.isFinite(y) || !Number.isFinite(z)) continue;
		if (x < min[0]) min[0] = x;
		if (y < min[1]) min[1] = y;
		if (z < min[2]) min[2] = z;
		if (x > max[0]) max[0] = x;
		if (y > max[1]) max[1] = y;
		if (z > max[2]) max[2] = z;
	}
	return { min, max, empty: min[0] > max[0] };
}

/** The box around a mesh box once placed by `matrix`: its eight corners moved. */
export function placedBounds(box, matrix) {
	const min = [Infinity, Infinity, Infinity];
	const max = [-Infinity, -Infinity, -Infinity];
	if (box.empty) return { min, max, empty: true };
	for (let i = 0; i < 8; i++) {
		const p = transformPoint(matrix, [
			i & 1 ? box.max[0] : box.min[0],
			i & 2 ? box.max[1] : box.min[1],
			i & 4 ? box.max[2] : box.min[2],
		]);
		for (let k = 0; k < 3; k++) {
			min[k] = Math.min(min[k], p[k]);
			max[k] = Math.max(max[k], p[k]);
		}
	}
	return { min, max, empty: false };
}

export class Renderer {
	#gl;
	#canvas;
	#mesh;
	#line;
	/** GPU buffers per mesh key. */
	#meshes = new Map();
	/** What to draw: `{ mesh, matrix, color, visible }`. */
	#items = [];
	#grid = null;
	#bounds = null;
	// The orbit camera: around `target`, Z up.
	#target = [0, 0, 0];
	#distance = 200;
	#azimuth = -Math.PI / 3;
	#elevation = Math.PI / 6;
	#frameRequested = false;

	/** @param {HTMLCanvasElement} canvas */
	constructor(canvas) {
		const gl = canvas.getContext('webgl2', { antialias: true });
		if (!gl) throw new Error('WebGL2 is not available');
		this.#gl = gl;
		this.#canvas = canvas;
		this.#mesh = compile(gl, MESH_VS, MESH_FS);
		this.#line = compile(gl, LINE_VS, LINE_FS);
		this.#controls();
		new ResizeObserver(() => this.redraw()).observe(canvas);
		this.#setGrid(null);
	}

	/** Uploads a mesh once, to be drawn by any number of items. */
	addMesh(key, vertices, triangles) {
		const gl = this.#gl;
		const vao = gl.createVertexArray();
		gl.bindVertexArray(vao);
		const positions = gl.createBuffer();
		gl.bindBuffer(gl.ARRAY_BUFFER, positions);
		gl.bufferData(gl.ARRAY_BUFFER, vertices, gl.STATIC_DRAW);
		const at = gl.getAttribLocation(this.#mesh, 'position');
		gl.enableVertexAttribArray(at);
		gl.vertexAttribPointer(at, 3, gl.FLOAT, false, 0, 0);
		const indices = gl.createBuffer();
		gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, indices);
		gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, triangles, gl.STATIC_DRAW);
		gl.bindVertexArray(null);
		this.#meshes.set(key, { vao, positions, indices, count: triangles.length, box: meshBounds(vertices) });
	}

	/** Drops every mesh and item. */
	clear() {
		const gl = this.#gl;
		for (const m of this.#meshes.values()) {
			gl.deleteVertexArray(m.vao);
			gl.deleteBuffer(m.positions);
			gl.deleteBuffer(m.indices);
		}
		this.#meshes.clear();
		this.#items = [];
		this.#bounds = null;
		this.#setGrid(null);
		this.redraw();
	}

	/** What to draw. Each item: `{ mesh, matrix (Float64Array(16)), color, visible }`. */
	setItems(items) {
		this.#items = items;
		this.redraw();
	}

	/** The box around every visible item, or null when there is none. */
	bounds() {
		const min = [Infinity, Infinity, Infinity];
		const max = [-Infinity, -Infinity, -Infinity];
		let any = false;
		for (const item of this.#items) {
			if (!item.visible) continue;
			const m = this.#meshes.get(item.mesh);
			if (!m) continue;
			const b = placedBounds(m.box, item.matrix);
			if (b.empty) continue;
			any = true;
			for (let k = 0; k < 3; k++) {
				min[k] = Math.min(min[k], b.min[k]);
				max[k] = Math.max(max[k], b.max[k]);
			}
		}
		return any ? { min, max } : null;
	}

	/** Points the camera at everything shown, and fits the grid around it. */
	frame() {
		const b = this.bounds();
		this.#bounds = b;
		this.#setGrid(b);
		if (b) {
			this.#target = [0, 1, 2].map((k) => (b.min[k] + b.max[k]) / 2);
			const radius = Math.max(1, Math.hypot(b.max[0] - b.min[0], b.max[1] - b.min[1], b.max[2] - b.min[2]) / 2);
			this.#distance = radius / Math.sin((35 * Math.PI) / 180 / 2) * 1.1;
		} else {
			this.#target = [0, 0, 0];
			this.#distance = 200;
		}
		this.#azimuth = -Math.PI / 3;
		this.#elevation = Math.PI / 6;
		this.redraw();
	}

	/** The grid: 10 mm lines, every fifth stronger, around `b` and the origin. */
	#setGrid(b) {
		const gl = this.#gl;
		const reach = b
			? Math.max(50, ...b.min.slice(0, 2).map(Math.abs), ...b.max.slice(0, 2).map(Math.abs)) * 1.2
			: 100;
		const half = Math.ceil(reach / (GRID_MM * GRID_MAJOR_EVERY)) * GRID_MM * GRID_MAJOR_EVERY;
		const data = [];
		const line = (a, c, color) => data.push(...a, ...color, ...c, ...color);
		for (let v = -half; v <= half; v += GRID_MM) {
			const major = Math.round(v / GRID_MM) % GRID_MAJOR_EVERY === 0;
			const xColor = v === 0 ? COLORS.axisY : major ? COLORS.gridMajor : COLORS.gridMinor;
			const yColor = v === 0 ? COLORS.axisX : major ? COLORS.gridMajor : COLORS.gridMinor;
			line([v, -half, 0], [v, half, 0], xColor);
			line([-half, v, 0], [half, v, 0], yColor);
		}
		if (this.#grid) {
			gl.deleteVertexArray(this.#grid.vao);
			gl.deleteBuffer(this.#grid.buffer);
		}
		const vao = gl.createVertexArray();
		gl.bindVertexArray(vao);
		const buffer = gl.createBuffer();
		gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
		gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(data), gl.STATIC_DRAW);
		const pos = gl.getAttribLocation(this.#line, 'position');
		const col = gl.getAttribLocation(this.#line, 'lineColor');
		gl.enableVertexAttribArray(pos);
		gl.vertexAttribPointer(pos, 3, gl.FLOAT, false, 24, 0);
		gl.enableVertexAttribArray(col);
		gl.vertexAttribPointer(col, 3, gl.FLOAT, false, 24, 12);
		gl.bindVertexArray(null);
		this.#grid = { vao, buffer, count: data.length / 6, half };
	}

	/** Asks for a new frame, drawn once before the next repaint. */
	redraw() {
		if (this.#frameRequested) return;
		this.#frameRequested = true;
		requestAnimationFrame(() => {
			this.#frameRequested = false;
			this.#draw();
		});
	}

	#eye() {
		const ce = Math.cos(this.#elevation);
		return [
			this.#target[0] + this.#distance * ce * Math.cos(this.#azimuth),
			this.#target[1] + this.#distance * ce * Math.sin(this.#azimuth),
			this.#target[2] + this.#distance * Math.sin(this.#elevation),
		];
	}

	#draw() {
		const gl = this.#gl;
		const canvas = this.#canvas;
		const ratio = Math.min(window.devicePixelRatio || 1, 2);
		const width = Math.max(1, Math.round(canvas.clientWidth * ratio));
		const height = Math.max(1, Math.round(canvas.clientHeight * ratio));
		if (canvas.width !== width || canvas.height !== height) {
			canvas.width = width;
			canvas.height = height;
		}
		gl.viewport(0, 0, width, height);
		gl.clearColor(...COLORS.ground, 1);
		gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
		gl.enable(gl.DEPTH_TEST);

		const eye = this.#eye();
		const span = this.#bounds
			? Math.hypot(...[0, 1, 2].map((k) => this.#bounds.max[k] - this.#bounds.min[k]))
			: 400;
		const far = this.#distance + 4 * Math.max(span, this.#grid.half * 2);
		const near = Math.max(far / 20000, this.#distance / 1000);
		const viewProjection = new Float32Array(
			multiply(perspective((35 * Math.PI) / 180, width / height, near, far), lookAt(eye, this.#target, [0, 0, 1])),
		);

		gl.useProgram(this.#line);
		gl.uniformMatrix4fv(gl.getUniformLocation(this.#line, 'viewProjection'), false, viewProjection);
		gl.bindVertexArray(this.#grid.vao);
		gl.drawArrays(gl.LINES, 0, this.#grid.count);

		gl.useProgram(this.#mesh);
		gl.uniformMatrix4fv(gl.getUniformLocation(this.#mesh, 'viewProjection'), false, viewProjection);
		gl.uniform3fv(gl.getUniformLocation(this.#mesh, 'eye'), eye);
		const modelAt = gl.getUniformLocation(this.#mesh, 'model');
		const colorAt = gl.getUniformLocation(this.#mesh, 'color');
		for (const item of this.#items) {
			const m = this.#meshes.get(item.mesh);
			if (!item.visible || !m || m.count === 0) continue;
			gl.uniformMatrix4fv(modelAt, false, new Float32Array(item.matrix));
			gl.uniform3fv(colorAt, item.color);
			gl.bindVertexArray(m.vao);
			gl.drawElements(gl.TRIANGLES, m.count, gl.UNSIGNED_INT, 0);
		}
		gl.bindVertexArray(null);
	}

	/** Mouse: left drag orbits, right or middle drag (or shift) pans, the wheel zooms, a double click frames. Touch: one finger orbits, two pinch and pan. */
	#controls() {
		const canvas = this.#canvas;
		const pointers = new Map();
		let pinch = null;
		canvas.style.touchAction = 'none';
		canvas.addEventListener('contextmenu', (e) => e.preventDefault());
		canvas.addEventListener('pointerdown', (e) => {
			canvas.setPointerCapture(e.pointerId);
			pointers.set(e.pointerId, { x: e.clientX, y: e.clientY, button: e.button, shift: e.shiftKey });
			pinch = null;
		});
		const end = (e) => {
			pointers.delete(e.pointerId);
			pinch = null;
		};
		canvas.addEventListener('pointerup', end);
		canvas.addEventListener('pointercancel', end);
		canvas.addEventListener('pointermove', (e) => {
			const p = pointers.get(e.pointerId);
			if (!p) return;
			const dx = e.clientX - p.x;
			const dy = e.clientY - p.y;
			p.x = e.clientX;
			p.y = e.clientY;
			if (pointers.size >= 2) {
				const [a, b] = [...pointers.values()];
				const gap = Math.hypot(a.x - b.x, a.y - b.y);
				const mid = [(a.x + b.x) / 2, (a.y + b.y) / 2];
				if (pinch) {
					this.#zoom(pinch.gap / Math.max(gap, 1));
					this.#pan(mid[0] - pinch.mid[0], mid[1] - pinch.mid[1]);
				}
				pinch = { gap, mid };
				return;
			}
			if (p.button === 0 && !p.shift && !e.shiftKey) this.#orbit(dx, dy);
			else this.#pan(dx, dy);
		});
		canvas.addEventListener(
			'wheel',
			(e) => {
				e.preventDefault();
				const lines = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
				this.#zoom(Math.exp((e.deltaY * lines) / 600));
			},
			{ passive: false },
		);
		canvas.addEventListener('dblclick', () => this.frame());
	}

	#orbit(dx, dy) {
		this.#azimuth -= dx * 0.008;
		this.#elevation = Math.max(-1.55, Math.min(1.55, this.#elevation + dy * 0.008));
		this.redraw();
	}

	#pan(dx, dy) {
		// One pixel moves the target by what a pixel spans at its distance.
		const perPixel = (2 * this.#distance * Math.tan((35 * Math.PI) / 360)) / Math.max(1, this.#canvas.clientHeight);
		const right = [-Math.sin(this.#azimuth), Math.cos(this.#azimuth), 0];
		const se = Math.sin(this.#elevation);
		const up = [-se * Math.cos(this.#azimuth), -se * Math.sin(this.#azimuth), Math.cos(this.#elevation)];
		for (let k = 0; k < 3; k++) this.#target[k] += (-dx * right[k] + dy * up[k]) * perPixel;
		this.redraw();
	}

	#zoom(factor) {
		this.#distance = Math.max(0.01, Math.min(1e7, this.#distance * factor));
		this.redraw();
	}
}
