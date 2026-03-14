//! 3D primitive generators

use super::vertex::Vertex3D;

pub fn cube_vertices() -> Vec<Vertex3D> {
    let positions = [
        // Front
        ([-0.5, -0.5, 0.5], [0.0, 0.0, 1.0], [0.0, 1.0]),
        ([0.5, -0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 1.0]),
        ([0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [1.0, 0.0]),
        ([-0.5, 0.5, 0.5], [0.0, 0.0, 1.0], [0.0, 0.0]),
        // Back
        ([-0.5, -0.5, -0.5], [0.0, 0.0, -1.0], [1.0, 1.0]),
        ([-0.5, 0.5, -0.5], [0.0, 0.0, -1.0], [1.0, 0.0]),
        ([0.5, 0.5, -0.5], [0.0, 0.0, -1.0], [0.0, 0.0]),
        ([0.5, -0.5, -0.5], [0.0, 0.0, -1.0], [0.0, 1.0]),
        // Top
        ([-0.5, 0.5, -0.5], [0.0, 1.0, 0.0], [0.0, 1.0]),
        ([-0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [0.0, 0.0]),
        ([0.5, 0.5, 0.5], [0.0, 1.0, 0.0], [1.0, 0.0]),
        ([0.5, 0.5, -0.5], [0.0, 1.0, 0.0], [1.0, 1.0]),
        // Bottom
        ([-0.5, -0.5, -0.5], [0.0, -1.0, 0.0], [1.0, 1.0]),
        ([0.5, -0.5, -0.5], [0.0, -1.0, 0.0], [0.0, 1.0]),
        ([0.5, -0.5, 0.5], [0.0, -1.0, 0.0], [0.0, 0.0]),
        ([-0.5, -0.5, 0.5], [0.0, -1.0, 0.0], [1.0, 0.0]),
        // Right
        ([0.5, -0.5, -0.5], [1.0, 0.0, 0.0], [1.0, 1.0]),
        ([0.5, 0.5, -0.5], [1.0, 0.0, 0.0], [1.0, 0.0]),
        ([0.5, 0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 0.0]),
        ([0.5, -0.5, 0.5], [1.0, 0.0, 0.0], [0.0, 1.0]),
        // Left
        ([-0.5, -0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 1.0]),
        ([-0.5, -0.5, 0.5], [-1.0, 0.0, 0.0], [1.0, 1.0]),
        ([-0.5, 0.5, 0.5], [-1.0, 0.0, 0.0], [1.0, 0.0]),
        ([-0.5, 0.5, -0.5], [-1.0, 0.0, 0.0], [0.0, 0.0]),
    ];

    positions
        .iter()
        .map(|(p, n, uv)| Vertex3D::new(*p, *n, *uv))
        .collect()
}

pub fn cube_indices() -> Vec<u16> {
    vec![
        0, 1, 2, 2, 3, 0, // front
        4, 5, 6, 6, 7, 4, // back
        8, 9, 10, 10, 11, 8, // top
        12, 13, 14, 14, 15, 12, // bottom
        16, 17, 18, 18, 19, 16, // right
        20, 21, 22, 22, 23, 20, // left
    ]
}

pub fn sphere_vertices(segments: u32, rings: u32) -> Vec<Vertex3D> {
    let mut vertices = Vec::new();
    let radius = 0.5_f32;

    for ring in 0..=rings {
        let theta = ring as f32 * std::f32::consts::PI / rings as f32;
        let sin_theta = theta.sin();
        let cos_theta = theta.cos();

        for segment in 0..=segments {
            let phi = segment as f32 * 2.0 * std::f32::consts::PI / segments as f32;
            let sin_phi = phi.sin();
            let cos_phi = phi.cos();

            let x = cos_phi * sin_theta;
            let y = cos_theta;
            let z = sin_phi * sin_theta;

            let position = [x * radius, y * radius, z * radius];
            let normal = [x, y, z];
            let uv = [segment as f32 / segments as f32, ring as f32 / rings as f32];

            vertices.push(Vertex3D::new(position, normal, uv));
        }
    }

    vertices
}

pub fn sphere_indices(segments: u32, rings: u32) -> Vec<u16> {
    let mut indices = Vec::new();

    for ring in 0..rings {
        for segment in 0..segments {
            let first = ring * (segments + 1) + segment;
            let second = first + segments + 1;

            indices.push(first as u16);
            indices.push(second as u16);
            indices.push((first + 1) as u16);

            indices.push((first + 1) as u16);
            indices.push(second as u16);
            indices.push((second + 1) as u16);
        }
    }

    indices
}
