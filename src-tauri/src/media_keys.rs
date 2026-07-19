#[cfg(target_os = "macos")]
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
#[cfg(target_os = "macos")]
use objc2_core_graphics::{CGEvent, CGEventTapLocation, CGPreflightPostEventAccess, CGRequestPostEventAccess};
#[cfg(target_os = "macos")]
use objc2_foundation::NSPoint;

pub fn permission_status() -> bool {
    #[cfg(target_os = "macos")]
    {
        CGPreflightPostEventAccess()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn request_permission() -> bool {
    #[cfg(target_os = "macos")]
    {
        CGRequestPostEventAccess()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

pub fn post_play_pause() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        if !permission_status() {
            return Err("permission_required".into());
        }
        // NX_KEYTYPE_PLAY. AppKit represents media keys as system-defined events.
        // Send exactly one down/up pair for one best-effort toggle.
        for key_state in [0x0a_i64, 0x0b_i64] {
            let data1 = (16_i64 << 16) | (key_state << 8);
            let event = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::SystemDefined,
                NSPoint { x: 0.0, y: 0.0 },
                NSEventModifierFlags::empty(),
                0.0,
                0,
                None,
                8,
                data1 as isize,
                -1,
            )
            .ok_or_else(|| "Could not create the macOS media-key event.".to_string())?;
            let cg_event = event
                .CGEvent()
                .ok_or_else(|| "Could not bridge the macOS media-key event.".to_string())?;
            CGEvent::post(CGEventTapLocation::HIDEventTap, Some(&cg_event));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("unsupported".into())
    }
}
