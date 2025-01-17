/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::{Ref, RefMut};

use euclid::default::Size2D;
use surfman::{Context, Device, Error, NativeWidget};

pub trait RenderingContext {
    fn context(&self) -> Ref<'_, Context>;
    fn context_mut(&self) -> RefMut<'_, Context>;
    fn device(&self) -> Ref<'_, Device>;
    fn device_mut(&self) -> RefMut<'_, Device>;
    fn resize(&self, size: Size2D<i32>) -> Result<(), Error>;
    fn present(&self) -> Result<(), Error>;
    fn bind_native_surface_to_context(&self, native_widget: NativeWidget) -> Result<(), Error>;
}
