pub mod models;
pub mod storage;

pub use models::{AppConfig, Profile, DEFAULT_PROFILE_ID};
pub use storage::{load, save};
