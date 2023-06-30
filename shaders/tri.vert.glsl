#version 450

#extension GL_EXT_control_flow_attributes : require

layout(binding = 0) readonly buffer Prima {
	uint prima_data[];
};

layout(location = 0) out vec3 frag_color;

vec4 decode_vec(uint offset) {
	float x = uintBitsToFloat(prima_data[offset + 0]);
	float y = uintBitsToFloat(prima_data[offset + 1]);
	float z = uintBitsToFloat(prima_data[offset + 2]);
	float w = uintBitsToFloat(prima_data[offset + 3]);
	return vec4(x, y, z, w);
}

mat4 decode_proj() {
	vec4 c0 = decode_vec(4 * 0);
	vec4 c1 = decode_vec(4 * 1);
	vec4 c2 = decode_vec(4 * 2);
	vec4 c3 = decode_vec(4 * 3);
	return mat4(c0, c1, c2, c3);
}

uint decode_type(uint id) {
	return (id >> 26) & 0x3F;
}

uint decode_corner(uint id) {
	return (id >> 24) & 0x3;
}

uint decode_offset(uint id) {
	return id & 0xFFFFFF;
}

vec4 decode_color(uint c) {
	vec4 v = vec4(
		(c >>  0) & 0xFF,
		(c >>  8) & 0xFF,
		(c >> 16) & 0xFF,
		(c >> 24) & 0xFF
	);
	return v / 255.0f;
}

void main() {
	uint id     = gl_VertexIndex;

	mat4 proj   = decode_proj();
	uint ptype  = decode_type(id);
	uint corner = decode_corner(id);
	uint offset = decode_offset(id);

	vec3 v;
	vec3 c;

	[[branch]]
		if (ptype == 0) {
			float vx = uintBitsToFloat(prima_data[offset + 0]);
			float vy = uintBitsToFloat(prima_data[offset + 1]);
			// @Idea Use unused corner bits as a mask to encode
			// color availability to compress single color tris
			// more.
			c = decode_color(prima_data[offset + 2]).rgb;
			v = vec3(vx, vy, 0.0);
		} else {
			vec4 r = decode_vec(offset);

			// @Speed Not sure if this optimizes well, can be rewritten
			// via some bit-twiddling.
			v = vec3(
				r.x + ((corner == 2 || corner == 3) ? r.z : 0),
				r.y + ((corner == 0 || corner == 3) ? r.w : 0),
				0
			);
			c = vec3(1.0, 0.7, 0.4);
		}

	v.xy = floor(v.xy + 0.5f);
	gl_Position = proj * vec4(v.xy, 0.0, 1.0);

	frag_color  = c;
}
