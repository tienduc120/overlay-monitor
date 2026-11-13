use super::SingleInstance;

#[test]
fn a_second_instance_is_refused_while_the_first_holds_the_name() {
    let first = SingleInstance::acquire("Local\\overlay-monitor-test-instance").unwrap();
    assert!(first.is_some());
    assert!(
        SingleInstance::acquire("Local\\overlay-monitor-test-instance")
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_name_is_free_again_once_the_first_instance_is_dropped() {
    let name = "Local\\overlay-monitor-test-instance-released";
    drop(SingleInstance::acquire(name).unwrap());

    let again = SingleInstance::acquire(name).unwrap();

    assert!(again.is_some(), "a closed instance must not keep the name");
}
