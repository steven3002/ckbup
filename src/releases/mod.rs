mod lookup;

#[cfg(test)]
pub use lookup::newest_stable;
pub use lookup::{latest_stable, release, stable_releases};
