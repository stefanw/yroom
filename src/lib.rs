use pyo3::prelude::*;

mod roomsync;

/// A Python module implemented in Rust.
#[pymodule]
fn yroom(_py: Python, m: &PyModule) -> PyResult<()> {
    pyo3_log::init();
    m.add_class::<roomsync::YRoomManager>()?;
    m.add_class::<roomsync::YRoomMessage>()?;
    Ok(())
}
