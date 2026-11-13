use super::{Repo, check_command, check_edit};

const REPO: Repo<'static> = Repo {
    root: "D:/repo",
    cwd: "D:/repo",
};

fn blocked(cmd: &str) -> bool {
    check_command(cmd, false, &REPO).is_some()
}

#[test]
fn ordinary_commands_pass() {
    for cmd in [
        "cargo nextest run -p xtask",
        "cargo clippy --workspace --all-targets",
        "cargo xtask verify",
        "git status && git diff --cached",
        "cat clippy.toml",
        "cargo update -p serde --precise 1.0.229",
        "docker compose up -d",
        "cargo test --doc -p platform-task",
        "cargo build 2>&1",
        "cargo build 2>\"$LOG\"",
        "echo hi > \"$OUT_FILE\"",
        "rm -rf target",
        "rm -rf services/billing",
        "rm -rf .xtask/stage",
        "rm D:/elsewhere/clippy.toml",
    ] {
        assert!(!blocked(cmd), "{cmd} should pass");
    }
}

#[test]
fn skipping_the_git_hooks_is_blocked() {
    for cmd in [
        "git commit --no-verify -m x",
        "git commit -nm x",
        "LEFTHOOK=0 git commit -m x",
        "$env:LEFTHOOK = '0'; git commit -m x",
        "git -c core.hooksPath=/dev/null commit -m x",
        "lefthook uninstall",
        "export LEFTHOOK=0; git commit -m x",
        "env LEFTHOOK=0 git commit -m x",
    ] {
        assert!(blocked(cmd), "{cmd} should be blocked");
    }
}

#[test]
fn a_commit_message_that_only_mentions_a_flag_is_not_running_it() {
    assert!(!blocked("git commit -m \"docs: explain why --no-verify is refused\""));
    assert!(!blocked("git commit -F - <<'EOF'\nci: never pass --no-verify\nEOF"));
}

#[test]
fn a_command_inside_a_shell_string_or_a_subshell_is_still_checked() {
    for cmd in [
        "sh -c 'rm clippy.toml'",
        "bash -c \"git commit -n -m x\"",
        "pwsh -Command \"Remove-Item deny.toml\"",
        "echo $(rm clippy.toml)",
        "{ rm clippy.toml; }",
    ] {
        assert!(blocked(cmd), "{cmd} should be blocked");
    }
}

#[test]
fn an_arithmetic_shift_is_not_mistaken_for_a_heredoc() {
    assert!(blocked("echo $((1<<2))\nrm clippy.toml"));
}

#[test]
fn commands_that_never_return_are_blocked_with_the_bounded_alternative() {
    for cmd in [
        "cargo test -p xtask",
        "cargo +nightly test",
        "cargo -q test -p xtask",
        "cargo run -p billing-api",
        "cargo run --bin billing-api",
        "cargo watch -x check",
        "docker compose up",
        "docker logs -f api",
        "kubectl logs --follow pod",
        "tail -f .xtask/logs/clippy.log",
        "Get-Content app.log -Wait",
    ] {
        assert!(blocked(cmd), "{cmd} should be blocked");
    }
    assert!(
        check_command("cargo test", false, &REPO).unwrap().contains("nextest"),
        "names the alternative"
    );
}

#[test]
fn updating_every_dependency_at_once_is_blocked() {
    assert!(blocked("cargo update"));
    assert!(blocked("cargo --locked update"));
}

#[test]
fn shell_writes_to_a_protected_path_are_blocked_unless_approved() {
    for cmd in [
        "rm clippy.toml",
        "echo x > deny.toml",
        "echo x 1>deny.toml",
        "echo x 2>>deny.toml",
        "echo x &>deny.toml",
        "echo x >| deny.toml",
        "sed -i s/deny/warn/ architecture.toml",
        "Remove-Item fixtures/violations/arch-layer -Recurse",
        "git rm xtask/src/gates/arch.rs",
        "Set-Content -Path .config/nextest.toml -Value x",
    ] {
        assert!(blocked(cmd), "{cmd} should be blocked");
        assert!(
            check_command(cmd, true, &REPO).is_none(),
            "{cmd} is allowed once the user approves"
        );
    }
}

#[test]
fn a_directory_holding_protected_files_or_a_path_that_reaches_one_is_protected_too() {
    for cmd in [
        "rm -rf xtask",
        "rm -rf xtask/src",
        "rm -rf .github",
        "mv .claude elsewhere",
        "rm -rf .",
        "rm -rf D:/",
        "rm *.toml",
        "Remove-Item fixtures/* -Recurse",
        "rm D:/repo/clippy.toml",
        "rm D:\\repo\\deny.toml",
        "rm xtask/../clippy.toml",
    ] {
        assert!(blocked(cmd), "{cmd} should be blocked");
    }
}

#[test]
fn a_protected_name_matches_whatever_its_case() {
    assert!(blocked("rm cargo.toml"));
    assert!(blocked("Remove-Item CLIPPY.TOML"));
    assert!(check_edit("cargo.toml", false, committed).is_some());
    assert!(check_edit("XTASK/src/gates/arch.rs", false, committed).is_some());
}

#[test]
fn a_relative_path_is_resolved_against_the_directory_the_command_runs_in() {
    let inside_xtask = Repo {
        root: "D:/repo",
        cwd: "D:/repo/xtask",
    };

    assert!(check_command("rm src/gates/arch.rs", false, &inside_xtask).is_some());
    assert!(
        check_command("rm clippy.toml", false, &inside_xtask).is_none(),
        "xtask/clippy.toml is not the root config"
    );
}

#[test]
fn approval_never_lifts_the_hook_bypass_or_the_hang_rules() {
    assert!(check_command("git commit --no-verify -m x", true, &REPO).is_some());
    assert!(check_command("cargo test", true, &REPO).is_some());
}

fn committed(_: &str) -> bool {
    true
}

#[test]
fn a_committed_protected_file_cannot_be_edited_unless_approved() {
    assert!(check_edit("clippy.toml", false, committed).is_some());
    assert!(check_edit("Cargo.toml", false, committed).is_some());
    assert!(check_edit("xtask/src/gates/guard.rs", false, committed).is_some());
    assert!(check_edit("fixtures/clean/platform/clock/src/lib.rs", false, committed).is_some());
    assert!(check_edit("clippy.toml", true, committed).is_none(), "approved");
}

#[test]
fn new_files_and_ordinary_files_can_be_edited() {
    assert!(
        check_edit("fixtures/violations/new-rule/rule.toml", false, |_| false).is_none(),
        "not committed yet"
    );
    assert!(
        check_edit("services/billing/domain/Cargo.toml", false, committed).is_none(),
        "only the root manifest"
    );
    assert!(check_edit("xtask/src/main.rs", false, committed).is_none());
}
