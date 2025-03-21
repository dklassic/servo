/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use dom_struct::dom_struct;
use euclid::default::Transform3D;
use js::rust::{CustomAutoRooterGuard, HandleObject};
use js::typedarray::{Float32Array, Float64Array};

use crate::dom::bindings::codegen::Bindings::DOMMatrixBinding::{DOMMatrixInit, DOMMatrixMethods};
use crate::dom::bindings::codegen::Bindings::DOMMatrixReadOnlyBinding::DOMMatrixReadOnlyMethods;
use crate::dom::bindings::codegen::UnionTypes::StringOrUnrestrictedDoubleSequence;
use crate::dom::bindings::error;
use crate::dom::bindings::error::Fallible;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::reflector::reflect_dom_object_with_proto;
use crate::dom::bindings::root::DomRoot;
use crate::dom::dommatrixreadonly::{
    DOMMatrixReadOnly, dommatrixinit_to_matrix, entries_to_matrix, transform_to_matrix,
};
use crate::dom::globalscope::GlobalScope;
use crate::dom::window::Window;
use crate::script_runtime::CanGc;

#[dom_struct]
pub(crate) struct DOMMatrix {
    parent: DOMMatrixReadOnly,
}

#[allow(non_snake_case)]
impl DOMMatrix {
    pub(crate) fn new(
        global: &GlobalScope,
        is2D: bool,
        matrix: Transform3D<f64>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new_with_proto(global, None, is2D, matrix, can_gc)
    }

    #[cfg_attr(crown, allow(crown::unrooted_must_root))]
    fn new_with_proto(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        is2D: bool,
        matrix: Transform3D<f64>,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        let dommatrix = Self::new_inherited(is2D, matrix);
        reflect_dom_object_with_proto(Box::new(dommatrix), global, proto, can_gc)
    }

    pub(crate) fn new_inherited(is2D: bool, matrix: Transform3D<f64>) -> Self {
        DOMMatrix {
            parent: DOMMatrixReadOnly::new_inherited(is2D, matrix),
        }
    }

    pub(crate) fn from_readonly(
        global: &GlobalScope,
        ro: &DOMMatrixReadOnly,
        can_gc: CanGc,
    ) -> DomRoot<Self> {
        Self::new(global, ro.is2D(), *ro.matrix(), can_gc)
    }
}

#[allow(non_snake_case)]
impl DOMMatrixMethods<crate::DomTypeHolder> for DOMMatrix {
    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-dommatrixreadonly
    fn Constructor(
        global: &GlobalScope,
        proto: Option<HandleObject>,
        can_gc: CanGc,
        init: Option<StringOrUnrestrictedDoubleSequence>,
    ) -> Fallible<DomRoot<Self>> {
        if init.is_none() {
            return Ok(Self::new_with_proto(
                global,
                proto,
                true,
                Transform3D::identity(),
                can_gc,
            ));
        }
        match init.unwrap() {
            StringOrUnrestrictedDoubleSequence::String(ref s) => {
                if !global.is::<Window>() {
                    return Err(error::Error::Type(
                        "String constructor is only supported in the main thread.".to_owned(),
                    ));
                }
                if s.is_empty() {
                    return Ok(Self::new(global, true, Transform3D::identity(), can_gc));
                }
                transform_to_matrix(s.to_string())
                    .map(|(is2D, matrix)| Self::new_with_proto(global, proto, is2D, matrix, can_gc))
            },
            StringOrUnrestrictedDoubleSequence::UnrestrictedDoubleSequence(ref entries) => {
                entries_to_matrix(&entries[..])
                    .map(|(is2D, matrix)| Self::new_with_proto(global, proto, is2D, matrix, can_gc))
            },
        }
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-frommatrix
    fn FromMatrix(
        global: &GlobalScope,
        other: &DOMMatrixInit,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<Self>> {
        dommatrixinit_to_matrix(other).map(|(is2D, matrix)| Self::new(global, is2D, matrix, can_gc))
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-fromfloat32array
    fn FromFloat32Array(
        global: &GlobalScope,
        array: CustomAutoRooterGuard<Float32Array>,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<DOMMatrix>> {
        let vec: Vec<f64> = array.to_vec().iter().map(|&x| x as f64).collect();
        DOMMatrix::Constructor(
            global,
            None,
            can_gc,
            Some(StringOrUnrestrictedDoubleSequence::UnrestrictedDoubleSequence(vec)),
        )
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-fromfloat64array
    fn FromFloat64Array(
        global: &GlobalScope,
        array: CustomAutoRooterGuard<Float64Array>,
        can_gc: CanGc,
    ) -> Fallible<DomRoot<DOMMatrix>> {
        let vec: Vec<f64> = array.to_vec();
        DOMMatrix::Constructor(
            global,
            None,
            can_gc,
            Some(StringOrUnrestrictedDoubleSequence::UnrestrictedDoubleSequence(vec)),
        )
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m11
    fn M11(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M11()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m11
    fn SetM11(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m11(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m12
    fn M12(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M12()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m12
    fn SetM12(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m12(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m13
    fn M13(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M13()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m13
    fn SetM13(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m13(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m14
    fn M14(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M14()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m14
    fn SetM14(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m14(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m21
    fn M21(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M21()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m21
    fn SetM21(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m21(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m22
    fn M22(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M22()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m22
    fn SetM22(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m22(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m23
    fn M23(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M23()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m23
    fn SetM23(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m23(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m24
    fn M24(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M24()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m24
    fn SetM24(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m24(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m31
    fn M31(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M31()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m31
    fn SetM31(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m31(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m32
    fn M32(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M32()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m32
    fn SetM32(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m32(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m33
    fn M33(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M33()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m33
    fn SetM33(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m33(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m34
    fn M34(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M34()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m34
    fn SetM34(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m34(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m41
    fn M41(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M41()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m41
    fn SetM41(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m41(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m42
    fn M42(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M42()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m42
    fn SetM42(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m42(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m43
    fn M43(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M43()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m43
    fn SetM43(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m43(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m44
    fn M44(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().M44()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-m44
    fn SetM44(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m44(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-a
    fn A(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().A()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-a
    fn SetA(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m11(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-b
    fn B(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().B()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-b
    fn SetB(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m12(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-c
    fn C(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().C()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-c
    fn SetC(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m21(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-d
    fn D(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().D()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-d
    fn SetD(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m22(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-e
    fn E(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().E()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-e
    fn SetE(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m41(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-f
    fn F(&self) -> f64 {
        self.upcast::<DOMMatrixReadOnly>().F()
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrixreadonly-f
    fn SetF(&self, value: f64) {
        self.upcast::<DOMMatrixReadOnly>().set_m42(value);
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-multiplyself
    fn MultiplySelf(&self, other: &DOMMatrixInit) -> Fallible<DomRoot<DOMMatrix>> {
        // Steps 1-3.
        self.upcast::<DOMMatrixReadOnly>()
            .multiply_self(other)
            // Step 4.
            .and(Ok(DomRoot::from_ref(self)))
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-premultiplyself
    fn PreMultiplySelf(&self, other: &DOMMatrixInit) -> Fallible<DomRoot<DOMMatrix>> {
        // Steps 1-3.
        self.upcast::<DOMMatrixReadOnly>()
            .pre_multiply_self(other)
            // Step 4.
            .and(Ok(DomRoot::from_ref(self)))
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-translateself
    fn TranslateSelf(&self, tx: f64, ty: f64, tz: f64) -> DomRoot<DOMMatrix> {
        // Steps 1-2.
        self.upcast::<DOMMatrixReadOnly>()
            .translate_self(tx, ty, tz);
        // Step 3.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-scaleself
    fn ScaleSelf(
        &self,
        scaleX: f64,
        scaleY: Option<f64>,
        scaleZ: f64,
        originX: f64,
        originY: f64,
        originZ: f64,
    ) -> DomRoot<DOMMatrix> {
        // Steps 1-6.
        self.upcast::<DOMMatrixReadOnly>()
            .scale_self(scaleX, scaleY, scaleZ, originX, originY, originZ);
        // Step 7.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-scale3dself
    fn Scale3dSelf(
        &self,
        scale: f64,
        originX: f64,
        originY: f64,
        originZ: f64,
    ) -> DomRoot<DOMMatrix> {
        // Steps 1-4.
        self.upcast::<DOMMatrixReadOnly>()
            .scale_3d_self(scale, originX, originY, originZ);
        // Step 5.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-rotateself
    fn RotateSelf(&self, rotX: f64, rotY: Option<f64>, rotZ: Option<f64>) -> DomRoot<DOMMatrix> {
        // Steps 1-7.
        self.upcast::<DOMMatrixReadOnly>()
            .rotate_self(rotX, rotY, rotZ);
        // Step 8.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-rotatefromvectorself
    fn RotateFromVectorSelf(&self, x: f64, y: f64) -> DomRoot<DOMMatrix> {
        // Step 1.
        self.upcast::<DOMMatrixReadOnly>()
            .rotate_from_vector_self(x, y);
        // Step 2.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-rotateaxisangleself
    fn RotateAxisAngleSelf(&self, x: f64, y: f64, z: f64, angle: f64) -> DomRoot<DOMMatrix> {
        // Steps 1-2.
        self.upcast::<DOMMatrixReadOnly>()
            .rotate_axis_angle_self(x, y, z, angle);
        // Step 3.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-skewxself
    fn SkewXSelf(&self, sx: f64) -> DomRoot<DOMMatrix> {
        // Step 1.
        self.upcast::<DOMMatrixReadOnly>().skew_x_self(sx);
        // Step 2.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-skewyself
    fn SkewYSelf(&self, sy: f64) -> DomRoot<DOMMatrix> {
        // Step 1.
        self.upcast::<DOMMatrixReadOnly>().skew_y_self(sy);
        // Step 2.
        DomRoot::from_ref(self)
    }

    // https://drafts.fxtf.org/geometry-1/#dom-dommatrix-invertself
    fn InvertSelf(&self) -> DomRoot<DOMMatrix> {
        // Steps 1-2.
        self.upcast::<DOMMatrixReadOnly>().invert_self();
        // Step 3.
        DomRoot::from_ref(self)
    }
}
