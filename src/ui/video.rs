//! Windows Media Foundation decodes local theme video on the background thread.
use image::RgbaImage;
use std::{path::Path, time::Duration};
use windows::{
    Win32::{Media::MediaFoundation::*, System::Com::*},
    core::{Error, HSTRING, Result},
};

pub fn play(path: &Path, send: impl Fn(RgbaImage, Duration) -> bool) -> Result<()> {
    // COM and Media Foundation are initialized and released on this decoder thread.
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
    }
    struct Com;
    impl Drop for Com {
        fn drop(&mut self) {
            unsafe { CoUninitialize() }
        }
    }
    let _com = Com;
    unsafe {
        MFStartup(MF_VERSION, MFSTARTUP_FULL)?;
    }
    struct Mf;
    impl Drop for Mf {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
            }
        }
    }
    let _mf = Mf;
    let path = HSTRING::from(path.as_os_str());
    let stream = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
    loop {
        // The reader owns the media source. Only decoded RGB32 frames are accepted.
        let reader = unsafe {
            let mut attrs = None;
            MFCreateAttributes(&mut attrs, 1)?;
            let attrs = attrs.ok_or_else(Error::empty)?;
            attrs.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)?;
            let reader = MFCreateSourceReaderFromURL(&path, &attrs)?;
            let media = MFCreateMediaType()?;
            media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
            media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
            reader.SetCurrentMediaType(stream, None, &media)?;
            reader
        };
        let mut seen = false;
        loop {
            let mut sample = None;
            let mut flags = 0;
            unsafe {
                reader.ReadSample(stream, 0, None, Some(&mut flags), None, Some(&mut sample))?;
            }
            if let Some(sample) = sample {
                let media = unsafe { reader.GetCurrentMediaType(stream)? };
                let size = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE)? };
                let (width, height) = ((size >> 32) as u32, size as u32);
                if width == 0 || height == 0 || width > 8192 || height > 8192 {
                    return Err(Error::empty());
                }
                let stride = unsafe { media.GetUINT32(&MF_MT_DEFAULT_STRIDE) }.map(|s| s as i32).unwrap_or(width as i32 * 4);
                let buffer = unsafe { sample.ConvertToContiguousBuffer()? };
                let mut ptr = std::ptr::null_mut();
                let mut count = 0;
                unsafe {
                    buffer.Lock(&mut ptr, None, Some(&mut count))?;
                }
                // Lock supplies count readable bytes until Unlock. Validate row geometry before slicing.
                let pitch = stride.unsigned_abs() as usize;
                let row = width as usize * 4;
                let copied = if !ptr.is_null() && pitch >= row && pitch.checked_mul(height as usize).is_some_and(|n| n <= count as usize) {
                    let bytes = unsafe { std::slice::from_raw_parts(ptr, count as usize) };
                    let mut rgba = Vec::with_capacity(row * height as usize);
                    for y in 0..height as usize {
                        let i = if stride < 0 { height as usize - 1 - y } else { y };
                        for px in bytes[i * pitch..i * pitch + row].as_chunks::<4>().0 {
                            rgba.extend_from_slice(&[px[2], px[1], px[0], 255]);
                        }
                    }
                    RgbaImage::from_raw(width, height, rgba)
                } else {
                    None
                };
                unsafe {
                    buffer.Unlock()?;
                }
                let img = copied.ok_or_else(Error::empty)?;
                let duration = unsafe { sample.GetSampleDuration() }.unwrap_or(333333).clamp(10000, 10000000);
                seen = true;
                if !send(img, Duration::from_micros(duration as u64 / 10)) {
                    return Ok(());
                }
            }
            if flags & (MF_SOURCE_READERF_ENDOFSTREAM.0 as u32) != 0 {
                break;
            }
        }
        if !seen {
            return Ok(());
        }
    }
}
