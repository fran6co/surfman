//! Surface management for Android and OpenHarmony using the `GraphicBuffer` class and EGL.

use crate::base::egl::ffi::EGLImageKHR;
use crate::context::ContextID;
use crate::egl::types::EGLSurface;
use crate::hardware_buffer::context::HardwareBufferContext;
use crate::hardware_buffer::device::HardwareBufferDevice;
use crate::renderbuffers::Renderbuffers;
use crate::{egl, BufferSurface, Context, Error, Surface, SurfaceID, SurfaceInfo, WindowSurface};

use crate::base::egl::device::EGL_FUNCTIONS;
use euclid::default::Size2D;
use glow::Texture;
use std::fmt::{self, Debug, Formatter};
use std::marker::PhantomData;
use std::thread;

#[cfg(android_platform)]
mod android_surface;

#[cfg(android_platform)]
pub use android_surface::*;

#[cfg(ohos_platform)]
mod ohos_surface;

#[cfg(ohos_platform)]
pub use ohos_surface::*;

/// An implementation of [`crate::WindowSurface`] for Android and OHOS platforms.
pub struct HardwareBufferWindowSurface {
    pub(crate) context_id: ContextID,
    pub(crate) size: Size2D<i32>,
    pub(crate) egl_surface: EGLSurface,
    pub(crate) destroyed: bool,
}

/// An implementation of [`crate::BufferSurface`] for Android and OHOS platforms.
pub struct HardwareBufferBufferSurface {
    pub(crate) context_id: ContextID,
    pub(crate) size: Size2D<i32>,
    pub(crate) hardware_buffer: *mut NativeBuffer,
    pub(crate) egl_image: EGLImageKHR,
    pub(crate) framebuffer_object: Option<glow::Framebuffer>,
    pub(crate) texture_object: Option<Texture>,
    pub(crate) renderbuffers: Renderbuffers,
    pub(crate) destroyed: bool,
}

/// A surface bound to a hardware buffer context.
pub(crate) enum HardwareBufferSurface {
    Window(HardwareBufferWindowSurface),
    Buffer(HardwareBufferBufferSurface),
}

/// An implementation of [`crate::SurfaceTexture`] for Android and OHOS platforms.
pub struct HardwareBufferSurfaceTexture {
    pub(crate) surface: HardwareBufferBufferSurface,
    pub(crate) local_egl_image: EGLImageKHR,
    pub(crate) texture_object: Option<Texture>,
    pub(crate) phantom: PhantomData<*const ()>,
}

unsafe impl Send for HardwareBufferWindowSurface {}
unsafe impl Send for HardwareBufferBufferSurface {}

pub(crate) fn window_surface(
    surface: &WindowSurface,
) -> Result<&HardwareBufferWindowSurface, Error> {
    #[cfg(android_platform)]
    return surface.android();
    #[cfg(ohos_platform)]
    return surface.ohos();
}

pub(crate) fn buffer_surface(
    surface: &BufferSurface,
) -> Result<&HardwareBufferBufferSurface, Error> {
    #[cfg(android_platform)]
    return surface.android();
    #[cfg(ohos_platform)]
    return surface.ohos();
}

impl TryFrom<Surface> for HardwareBufferSurface {
    type Error = (Error, Surface);
    fn try_from(surface: Surface) -> Result<HardwareBufferSurface, Self::Error> {
        match surface {
            Surface::Window(surface) => surface
                .try_into()
                .map(HardwareBufferSurface::Window)
                .map_err(|(error, surface)| (error, Surface::Window(surface))),
            Surface::Buffer(surface) => surface
                .try_into()
                .map(HardwareBufferSurface::Buffer)
                .map_err(|(error, surface)| (error, Surface::Buffer(surface))),
        }
    }
}

impl From<HardwareBufferSurface> for Surface {
    fn from(surface: HardwareBufferSurface) -> Surface {
        match surface {
            HardwareBufferSurface::Window(surface) => Surface::Window(surface.into()),
            HardwareBufferSurface::Buffer(surface) => Surface::Buffer(surface.into()),
        }
    }
}

impl Debug for HardwareBufferWindowSurface {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "Surface({:x})", self.id().0)
    }
}

impl Debug for HardwareBufferBufferSurface {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        write!(formatter, "Surface({:x})", self.id().0)
    }
}

impl Drop for HardwareBufferWindowSurface {
    fn drop(&mut self) {
        if !self.destroyed && !thread::panicking() {
            panic!("Should have destroyed the surface first with `destroy_surface()`!")
        }
    }
}

impl Drop for HardwareBufferBufferSurface {
    fn drop(&mut self) {
        if !self.destroyed && !thread::panicking() {
            panic!("Should have destroyed the surface first with `destroy_surface()`!")
        }
    }
}

impl HardwareBufferWindowSurface {
    pub(crate) fn info(&self) -> SurfaceInfo {
        SurfaceInfo {
            size: self.size,
            id: self.id(),
            context_id: self.context_id,
            framebuffer_object: None,
        }
    }

    pub(crate) fn id(&self) -> SurfaceID {
        SurfaceID(self.egl_surface as usize)
    }
}

impl HardwareBufferBufferSurface {
    pub(crate) fn info(&self) -> SurfaceInfo {
        SurfaceInfo {
            size: self.size,
            id: self.id(),
            context_id: self.context_id,
            framebuffer_object: self.framebuffer_object,
        }
    }

    pub(crate) fn id(&self) -> SurfaceID {
        SurfaceID(self.egl_image as usize)
    }
}

impl HardwareBufferSurface {
    pub(crate) fn context_id(&self) -> ContextID {
        match self {
            HardwareBufferSurface::Window(surface) => surface.context_id,
            HardwareBufferSurface::Buffer(surface) => surface.context_id,
        }
    }

    pub(crate) fn resize(&mut self, size: Size2D<i32>) {
        match self {
            HardwareBufferSurface::Window(surface) => surface.size = size,
            HardwareBufferSurface::Buffer(surface) => surface.size = size,
        }
    }

    pub(crate) fn info(&self) -> SurfaceInfo {
        match self {
            HardwareBufferSurface::Window(surface) => surface.info(),
            HardwareBufferSurface::Buffer(surface) => surface.info(),
        }
    }
}

impl Debug for HardwareBufferSurfaceTexture {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        write!(f, "SurfaceTexture({:?})", self.surface)
    }
}

impl HardwareBufferDevice {
    /// Displays the contents of a widget surface on screen.
    ///
    /// Widget surfaces are internally double-buffered, so changes to them don't show up in their
    /// associated widgets until this method is called.
    ///
    /// The supplied context must match the context the surface was created with, or an
    /// `IncompatibleSurface` error is returned.
    pub fn present_surface(
        &self,
        context: &Context,
        surface: &mut WindowSurface,
    ) -> Result<(), Error> {
        self.present_surface_inner(context.hardware_buffer()?, window_surface(surface)?)
    }

    pub(crate) fn present_surface_inner(
        &self,
        context: &HardwareBufferContext,
        surface: &HardwareBufferWindowSurface,
    ) -> Result<(), Error> {
        if context.id != surface.context_id {
            return Err(Error::IncompatibleSurface);
        }

        EGL_FUNCTIONS.with(|egl| unsafe {
            egl.SwapBuffers(self.egl_display, surface.egl_surface);
            Ok(())
        })
    }

    /// Resizes a widget surface.
    pub fn resize_surface(
        &self,
        _context: &Context,
        surface: &mut WindowSurface,
        size: Size2D<i32>,
    ) -> Result<(), Error> {
        let surface: &mut HardwareBufferWindowSurface = surface.try_into()?;
        surface.size = size;
        Ok(())
    }

    pub(crate) fn destroy_window_surface(
        &self,
        context: &mut HardwareBufferContext,
        surface: &mut HardwareBufferWindowSurface,
    ) -> Result<(), Error> {
        if context.id != surface.context_id {
            return Err(Error::IncompatibleSurface);
        }

        EGL_FUNCTIONS.with(|egl| unsafe {
            egl.DestroySurface(self.egl_display, surface.egl_surface);
            surface.egl_surface = egl::NO_SURFACE;
        });

        surface.destroyed = true;
        Ok(())
    }
}
