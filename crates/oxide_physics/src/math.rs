//! Math wrappers retained for API stability in the in-house backend.

use glam::{Quat, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct NaVec3(pub Vec3);

impl NaVec3 {
    pub fn into_inner(self) -> Vec3 {
        self.0
    }
}

impl From<Vec3> for NaVec3 {
    fn from(value: Vec3) -> Self {
        Self(value)
    }
}

impl From<&Vec3> for NaVec3 {
    fn from(value: &Vec3) -> Self {
        Self(*value)
    }
}

impl From<NaVec3> for Vec3 {
    fn from(value: NaVec3) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct NaQuat(pub Quat);

impl NaQuat {
    pub fn into_inner(self) -> Quat {
        self.0
    }
}

impl From<Quat> for NaQuat {
    fn from(value: Quat) -> Self {
        Self(value)
    }
}

impl From<&Quat> for NaQuat {
    fn from(value: &Quat) -> Self {
        Self(*value)
    }
}

impl From<NaQuat> for Quat {
    fn from(value: NaQuat) -> Self {
        value.0
    }
}
