//! Draws scenes without a window or a display server, into images: the wgpu
//! (Linux) counterpart of GPUI's headless Metal renderer and of the DirectX
//! one in ibisOffice's fork of gpui-pre-windows, with the same drawing as a
//! window's renderer.
//!
//! Added in ibisOffice's fork of gpui-pre-wgpu 0.3.6.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui::{DevicePixels, PlatformAtlas, PlatformHeadlessRenderer, Scene, Size};

use crate::{WgpuContext, WgpuRenderer};

/// The format of the texture drawn into: its bytes are an `RgbaImage`'s.
pub(crate) const HEADLESS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// The most bytes read back from the GPU at once. A picture larger than this
/// (a sheet of a workbook printed at a high resolution) is read in bands, so
/// that no buffer passes the device's limit.
const READBACK_BYTES: u64 = 64 * 1024 * 1024;

pub struct WgpuHeadlessRenderer {
    renderer: WgpuRenderer,
    /// The texture drawn into and its view, of the size last drawn.
    target: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Kept as long as the renderer, whose device is this context's.
    _context: WgpuContext,
}

impl WgpuHeadlessRenderer {
    /// A renderer on the system's GPU, or on a software one (lavapipe,
    /// llvmpipe) on a machine without a GPU.
    pub fn new() -> Result<Self> {
        let context = WgpuContext::new_headless().context("Creating a wgpu device")?;
        let renderer = WgpuRenderer::new_headless(&context)?;
        Ok(Self {
            renderer,
            target: None,
            _context: context,
        })
    }

    /// Draw `scene` into the target texture, made anew if `size` changed.
    fn draw(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<()> {
        let width = size.width.0.max(1) as u32;
        let height = size.height.0.max(1) as u32;
        let limit = self.renderer.max_texture_size();
        anyhow::ensure!(
            width <= limit && height <= limit,
            "a picture of {width}x{height} pixels is larger than this GPU draws ({limit})"
        );
        // Also drops the textures paths are drawn through, which are of the
        // size drawn.
        self.renderer.update_drawable_size(size);

        let stale = self
            .target
            .as_ref()
            .is_none_or(|(texture, _)| texture.width() != width || texture.height() != height);
        if stale {
            if let Some((texture, _)) = self.target.take() {
                texture.destroy();
            }
            let (device, _) = self.renderer.headless_parts();
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("headless_target"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: HEADLESS_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some((texture, view));
        }

        // As a window's renderer does before a frame: the glyphs and pictures
        // rasterized since the last one go to their atlas textures.
        self.renderer.sprite_atlas().before_frame();
        let (_, view) = self.target.as_ref().expect("the target was just made");
        let drawn = self.renderer.draw_to_view(scene, view);
        if let Some(error) = self.renderer.take_error() {
            anyhow::bail!("GPU error: {error}");
        }
        anyhow::ensure!(drawn, "the scene could not be drawn");
        Ok(())
    }

    /// The pixels of the target texture.
    fn read(&self) -> Result<image::RgbaImage> {
        let (texture, _) = self.target.as_ref().context("nothing was drawn")?;
        let (device, queue) = self.renderer.headless_parts();
        let (width, height) = (texture.width(), texture.height());
        let row = 4 * width as usize;
        // Rows of a buffer copied to start at multiples of the alignment.
        let padded = (4 * width).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let band = ((READBACK_BYTES / u64::from(padded)) as u32).clamp(1, height);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("headless_readback"),
            size: u64::from(padded) * u64::from(band),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let mut pixels = Vec::with_capacity(row * height as usize);
        let mut top = 0;
        while top < height {
            let rows = band.min(height - top);
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("headless_readback"),
            });
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: top, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded),
                        rows_per_image: Some(rows),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height: rows,
                    depth_or_array_layers: 1,
                },
            );
            queue.submit(std::iter::once(encoder.finish()));

            let slice = buffer.slice(..u64::from(padded) * u64::from(rows));
            let (sender, receiver) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                sender.send(result).ok();
            });
            device
                .poll(wgpu::PollType::Wait {
                    submission_index: None,
                    timeout: None,
                })
                .map_err(|error| anyhow::anyhow!("Waiting for the GPU: {error:?}"))?;
            receiver
                .recv()
                .context("the GPU did not answer")?
                .map_err(|error| anyhow::anyhow!("Reading the picture back: {error}"))?;
            {
                let data = slice.get_mapped_range();
                for line in data.chunks(padded as usize).take(rows as usize) {
                    pixels.extend_from_slice(&line[..row]);
                }
            }
            buffer.unmap();
            top += rows;
        }
        if let Some(error) = self.renderer.take_error() {
            anyhow::bail!("GPU error: {error}");
        }

        // Drawn as an opaque window is: what is under its pixels does not
        // show, whatever alpha the blending left in them.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel[3] = 0xff;
        }
        image::RgbaImage::from_raw(width, height, pixels).context("the picture read is not whole")
    }
}

impl PlatformHeadlessRenderer for WgpuHeadlessRenderer {
    fn render_scene_to_image(
        &mut self,
        scene: &Scene,
        size: Size<DevicePixels>,
    ) -> Result<image::RgbaImage> {
        self.draw(scene, size)?;
        self.read()
    }

    fn render_scene(&mut self, scene: &Scene, size: Size<DevicePixels>) -> Result<()> {
        self.draw(scene, size)
    }

    fn sprite_atlas(&self) -> Arc<dyn PlatformAtlas> {
        self.renderer.sprite_atlas().clone()
    }
}
