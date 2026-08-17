/// Return the default bounded in-flight window for a worker count.
pub const fn in_flight_window(worker_count: usize) -> usize {
    let window = worker_count.saturating_mul(2);
    if window == 0 { 1 } else { window }
}

#[cfg(test)]
mod tests {
    use super::in_flight_window;

    #[test]
    fn in_flight_window_is_twice_the_worker_count() {
        assert_eq!(in_flight_window(0), 1);
        assert_eq!(in_flight_window(4), 8);
        assert_eq!(in_flight_window(usize::MAX), usize::MAX);
    }
}
