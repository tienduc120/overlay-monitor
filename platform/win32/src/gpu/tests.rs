use super::{gpu_adapters, gpu_perf_data};

#[test]
#[ignore = "needs a GPU (the hosted CI runner has none)"]
fn at_least_one_adapter_with_a_name_is_listed() {
    let list = gpu_adapters().unwrap();
    assert!(list.iter().any(|a| !a.name.is_empty()), "{list:?}");
}

#[test]
#[ignore = "needs a GPU (the hosted CI runner has none)"]
fn perf_data_has_a_plausible_temperature_for_some_adapter() {
    let list = gpu_adapters().unwrap();
    let plausible = list
        .iter()
        .any(|a| gpu_perf_data(a.luid).is_ok_and(|perf| (200..=1000).contains(&perf.temperature_deci_c)));
    assert!(plausible, "{list:?}");
}
