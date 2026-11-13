use super::CounterArray;

#[test]
fn the_second_collect_names_every_gpu_engine_instance() {
    let mut counter = CounterArray::open(r"\GPU Engine(*)\Utilization Percentage").unwrap();
    assert!(
        counter.collect().unwrap().is_empty(),
        "the first collect only primes the rate"
    );
    // PDH needs time between two collects to compute a rate; a channel nobody sends on is a plain timed wait.
    let (_keep_open, idle) = std::sync::mpsc::channel::<()>();
    idle.recv_timeout(std::time::Duration::from_millis(300)).unwrap_err();
    let samples = counter.collect().unwrap();
    assert!(
        samples.iter().any(|(name, _)| name.contains("engtype_")),
        "{} samples, first: {:?}",
        samples.len(),
        samples.first()
    );
}
