use super::gpu_adapters;

#[test]
#[ignore = "needs a GPU (the hosted CI runner has none)"]
fn at_least_one_adapter_with_a_name_is_listed() {
    let list = gpu_adapters().unwrap();
    assert!(list.iter().any(|a| !a.name.is_empty()), "{list:?}");
}
