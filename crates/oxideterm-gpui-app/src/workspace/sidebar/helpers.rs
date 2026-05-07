fn titlebar_background(panel: u32, active: u32, accent: u32) -> u32 {
    let base = mix_rgb(panel, active, 0.42);
    mix_rgb(base, accent, 0.08)
}

fn active_session_readiness(readiness: &NodeReadiness) -> ActiveSessionReadiness {
    match readiness {
        NodeReadiness::Ready => ActiveSessionReadiness::Ready,
        NodeReadiness::Connecting => ActiveSessionReadiness::Connecting,
        NodeReadiness::Error => ActiveSessionReadiness::Error,
        NodeReadiness::Disconnected => ActiveSessionReadiness::Disconnected,
    }
}

fn titlebar_button_hover(background: u32) -> u32 {
    if relative_luminance(background) > 0.45 {
        mix_rgb(background, 0x000000, 0.10)
    } else {
        mix_rgb(background, 0xffffff, 0.12)
    }
}

fn readable_color(background: u32, preferred: u32, fallback: u32) -> u32 {
    if contrast_ratio(background, preferred) >= 3.0 {
        preferred
    } else {
        fallback
    }
}

fn mix_rgb(a: u32, b: u32, amount: f32) -> u32 {
    let amount = amount.clamp(0.0, 1.0);
    let mix = |shift: u32| {
        let left = ((a >> shift) & 0xffu32) as f32;
        let right = ((b >> shift) & 0xffu32) as f32;
        (left + (right - left) * amount).round().clamp(0.0, 255.0) as u32
    };
    (mix(16) << 16) | (mix(8) << 8) | mix(0)
}

fn contrast_ratio(a: u32, b: u32) -> f32 {
    let l1 = relative_luminance(a);
    let l2 = relative_luminance(b);
    let (light, dark) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
    (light + 0.05) / (dark + 0.05)
}

fn relative_luminance(color: u32) -> f32 {
    let channel = |shift: u32| {
        let value = ((color >> shift) & 0xffu32) as f32 / 255.0;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    channel(16) * 0.2126 + channel(8) * 0.7152 + channel(0) * 0.0722
}
