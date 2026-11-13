//! Unit tests for `activate.rs`, kept beside it rather than inside it.
//!
//! The code is asserted whole, for each shell, because it is run as it is
//! printed: a quote out of place is code somebody's shell runs.

use super::*;

/// A root with a space and both quotes in it, named as its directory is.
const ROOT: &str = r#"/home/you/it's a "weaver""#;
const NAME: &str = r#"it's a "weaver""#;

/// What `ROOT` activates in bash and in zsh, which share it.
const BOURNE: &str = r#"if typeset -f flayer-deactivate >/dev/null 2>&1; then
    flayer-deactivate
fi
if typeset -f mindweaver-deactivate >/dev/null 2>&1; then
    mindweaver-deactivate
fi
if [ -n "${MINDWEAVER_WORKSPACE+set}" ]; then
    _WEAVER_OLD_WORKSPACE="$MINDWEAVER_WORKSPACE"
else
    unset _WEAVER_OLD_WORKSPACE
fi
export MINDWEAVER_WORKSPACE='/home/you/it'\''s a "weaver"'
_WEAVER_PROMPT='(weaver:it'\''s a "weaver") '
PS1="$_WEAVER_PROMPT${PS1-}"
if [ -n "${_OLD_VIRTUAL_PS1+set}" ]; then
    _OLD_VIRTUAL_PS1="$_WEAVER_PROMPT$_OLD_VIRTUAL_PS1"
fi
mindweaver-deactivate() {
    if [ -n "${_WEAVER_OLD_WORKSPACE+set}" ]; then
        export MINDWEAVER_WORKSPACE="$_WEAVER_OLD_WORKSPACE"
        unset _WEAVER_OLD_WORKSPACE
    else
        unset MINDWEAVER_WORKSPACE
    fi
    PS1="${PS1/"$_WEAVER_PROMPT"/}"
    if [ -n "${_OLD_VIRTUAL_PS1+set}" ]; then
        _OLD_VIRTUAL_PS1="${_OLD_VIRTUAL_PS1/"$_WEAVER_PROMPT"/}"
    fi
    unset _WEAVER_PROMPT
    unset -f mindweaver-deactivate
}
"#;

#[test]
fn bash_and_zsh_are_given_one_code_with_every_quote_escaped() {
    assert_eq!(written(Shell::Bash, ROOT, &prompt(NAME)), BOURNE);
    assert_eq!(written(Shell::Zsh, ROOT, &prompt(NAME)), BOURNE);
}

#[test]
fn fish_is_given_its_own_code_with_its_two_escapes() {
    let root = r"/home/you/it's a \weaver\";
    let name = r#"it's a "weaver""#;

    assert_eq!(
        written(Shell::Fish, root, &prompt(name)),
        r#"if functions -q flayer-deactivate
    flayer-deactivate
end
if functions -q mindweaver-deactivate
    mindweaver-deactivate
end
if set -q MINDWEAVER_WORKSPACE
    set -g _weaver_old_workspace $MINDWEAVER_WORKSPACE
else
    set -e _weaver_old_workspace
end
set -gx MINDWEAVER_WORKSPACE '/home/you/it\'s a \\weaver\\'
set -g _weaver_prompt '(weaver:it\'s a "weaver") '
if not functions -q _weaver_old_fish_prompt
    functions -c fish_prompt _weaver_old_fish_prompt
    function fish_prompt
        set -l last_status $status
        if set -q _weaver_prompt
            printf '%s' $_weaver_prompt
        end
        echo "exit $last_status" | source
        _weaver_old_fish_prompt
    end
end
function mindweaver-deactivate
    if set -q _weaver_old_workspace
        set -gx MINDWEAVER_WORKSPACE $_weaver_old_workspace
        set -e _weaver_old_workspace
    else
        set -e MINDWEAVER_WORKSPACE
    end
    set -e _weaver_prompt
    functions -e mindweaver-deactivate
end
"#
    );
}

#[test]
fn powershell_is_given_its_own_code_with_quotes_doubled() {
    assert_eq!(
        written(Shell::Pwsh, ROOT, &prompt(NAME)),
        r#"if (Test-Path -Path Function:flayer-deactivate) {
    flayer-deactivate
}
if (Test-Path -Path Function:mindweaver-deactivate) {
    mindweaver-deactivate
}
if (Test-Path -Path Env:MINDWEAVER_WORKSPACE) {
    $global:_WeaverOldWorkspace = $Env:MINDWEAVER_WORKSPACE
} else {
    Remove-Variable -Name _WeaverOldWorkspace -Scope Global -ErrorAction SilentlyContinue
}
$Env:MINDWEAVER_WORKSPACE = '/home/you/it''s a "weaver"'
$global:_WeaverPrompt = '(weaver:it''s a "weaver") '
if (-not (Get-Variable -Name _WeaverOldPrompt -Scope Global -ErrorAction SilentlyContinue)) {
    $global:_WeaverOldPrompt = $Function:prompt
    function global:prompt {
        Write-Host -NoNewline $global:_WeaverPrompt
        & $global:_WeaverOldPrompt
    }
}
function global:mindweaver-deactivate {
    $old = Get-Variable -Name _WeaverOldWorkspace -Scope Global -ErrorAction SilentlyContinue
    if ($old) {
        $Env:MINDWEAVER_WORKSPACE = $old.Value
        Remove-Variable -Name _WeaverOldWorkspace -Scope Global
    } else {
        Remove-Item -Path Env:MINDWEAVER_WORKSPACE -ErrorAction SilentlyContinue
    }
    $global:_WeaverPrompt = ''
    Remove-Item -Path Function:mindweaver-deactivate
}
"#
    );
}

#[test]
fn powershell_doubles_the_typographic_single_quotes_it_also_ends_strings_on() {
    assert_eq!(
        pwsh_quoted("a\u{2018}b\u{2019}c\u{201A}d\u{201B}e'f"),
        "'a\u{2018}\u{2018}b\u{2019}\u{2019}c\u{201A}\u{201A}d\u{201B}\u{201B}e''f'"
    );
}

#[test]
fn a_path_full_of_what_a_shell_expands_is_one_word_and_nothing_more() {
    let root = "/tmp/$(touch pwned) `id` ${HOME} \\n !! *";

    assert_eq!(
        single_quoted(root),
        "'/tmp/$(touch pwned) `id` ${HOME} \\n !! *'"
    );
    assert_eq!(
        fish_quoted(root),
        "'/tmp/$(touch pwned) `id` ${HOME} \\\\n !! *'"
    );
    assert_eq!(
        pwsh_quoted(root),
        "'/tmp/$(touch pwned) `id` ${HOME} \\n !! *'"
    );
}

#[test]
fn the_prompt_shows_what_a_prompt_would_act_on_as_an_underscore() {
    assert_eq!(prompt("writing"), "(weaver:writing) ");
    assert_eq!(prompt("café notes"), "(weaver:café notes) ");
    assert_eq!(
        prompt("$(touch pwned) `id` %n !! \\w\nx\u{1b}"),
        "(weaver:_(touch pwned) _id_ _n __ _w_x_) "
    );
}

#[test]
fn every_shell_sets_the_variable_and_defines_its_own_deactivate_never_the_plain_one() {
    for shell in Shell::ALL {
        let code = written(shell, "/w", &prompt("w"));

        assert!(code.contains(WORKSPACE_VARIABLE), "{shell}:\n{code}");
        assert!(code.contains(DEACTIVATE), "{shell}:\n{code}");
        // Every `deactivate` it names is its own, or the one a Mindflayer
        // build defined, which it only runs: the plain one is an active
        // virtualenv's, and redefining it would take it away.
        for (at, word) in code.match_indices("deactivate") {
            let named = &code[..at + word.len()];
            assert!(
                named.ends_with(DEACTIVATE) || named.ends_with(FLAYER_DEACTIVATE),
                "{shell}:\n{code}"
            );
        }
        // Nothing of the old activation's is set or defined, only undone.
        assert!(
            !code.contains(FLAYER_WORKSPACE_VARIABLE),
            "{shell}:\n{code}"
        );
        assert_eq!(
            code.matches(FLAYER_DEACTIVATE).count(),
            2,
            "asked about and run, once each: {shell}:\n{code}"
        );
    }
}

#[test]
fn a_shell_is_named_by_its_name_and_guessed_from_its_program() {
    for shell in Shell::ALL {
        assert_eq!(shell.name().parse::<Shell>(), Ok(shell));
        assert_eq!(Shell::from_program(shell.name()), Some(shell));
    }
    assert_eq!(Shell::from_program("/bin/bash"), Some(Shell::Bash));
    assert_eq!(Shell::from_program("/usr/local/bin/zsh"), Some(Shell::Zsh));
    assert_eq!(Shell::from_program("/usr/bin/fish"), Some(Shell::Fish));
    assert_eq!(
        Shell::from_program("/opt/microsoft/pwsh"),
        Some(Shell::Pwsh)
    );
    assert_eq!(Shell::from_program("powershell.exe"), Some(Shell::Pwsh));
    assert_eq!(Shell::from_program("/bin/tcsh"), None);
    assert_eq!(Shell::from_program("/bin/sh"), None);
    assert_eq!(Shell::from_program(""), None);
}

#[test]
fn a_word_that_is_no_shell_says_which_are() {
    assert_eq!(
        "tcsh".parse::<Shell>().unwrap_err().to_string(),
        "unknown shell `tcsh`; expected bash, zsh, fish or pwsh"
    );
}

#[test]
fn a_workspace_not_made_yet_is_not_activated() {
    let home = tempfile::TempDir::new().unwrap();
    let unmade = WeaverWorkspace::unmade(home.path()).unwrap();

    let error = script(&unmade, Shell::Bash).unwrap_err();

    assert!(matches!(error, ActivateError::Unmade { .. }), "{error}");
}

#[test]
fn a_made_workspace_is_activated_by_its_root_and_named_by_its_marker() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join("writing");
    let (workspace, _) = WeaverWorkspace::init(&root).unwrap();

    let code = script(&workspace, Shell::Bash).unwrap();

    let root = root.to_str().unwrap();
    assert_eq!(code, written(Shell::Bash, root, "(weaver:writing) "));
}

/// Linux alone, whose file systems take any bytes in a name, where macOS's
/// refuse one that is not UTF-8.
#[cfg(target_os = "linux")]
#[test]
fn a_path_that_is_not_utf8_is_refused_rather_than_mangled() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path().join(OsStr::from_bytes(b"not-\xffutf8"));
    let (workspace, _) = WeaverWorkspace::init(&root).unwrap();

    let error = script(&workspace, Shell::Bash).unwrap_err();

    assert!(matches!(error, ActivateError::NotUnicode { .. }), "{error}");
}
