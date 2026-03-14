//! Event types for the engine

use winit::event::WindowEvent;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    Resized { width: u32, height: u32 },
    ScaleFactorChanged { scale_factor: f64 },
    CloseRequested,
    Destroyed,
    Focused(bool),
}

pub fn window_event_to_engine(event: &WindowEvent) -> Option<EngineEvent> {
    match event {
        WindowEvent::Resized(size) => Some(EngineEvent::Resized {
            width: size.width,
            height: size.height,
        }),
        WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            Some(EngineEvent::ScaleFactorChanged {
                scale_factor: *scale_factor,
            })
        }
        WindowEvent::CloseRequested => Some(EngineEvent::CloseRequested),
        WindowEvent::Destroyed => Some(EngineEvent::Destroyed),
        WindowEvent::Focused(focused) => Some(EngineEvent::Focused(*focused)),
        _ => None,
    }
}
