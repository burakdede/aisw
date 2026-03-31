#![allow(dead_code, unused_imports)]
mod common;

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use common::TestEnv;
use predicates::str::contains;

fn add_fake_security_tool(env: &TestEnv) {
    env.add_script_tool(
        "security",
        "#!/bin/sh\n\
         store_root=\"$HOME/keychain\"\n\
         service_path() {\n\
           printf '%s/%s' \"$store_root\" \"$1\"\n\
         }\n\
         item_dir() {\n\
           printf '%s/%s' \"$(service_path \"$1\")\" \"$2\"\n\
         }\n\
         first_item_dir() {\n\
           dir=\"$(service_path \"$1\")\"\n\
           [ -d \"$dir\" ] || return 1\n\
           for item in \"$dir\"/*; do\n\
             [ -d \"$item\" ] || continue\n\
             printf '%s' \"$item\"\n\
             return 0\n\
           done\n\
           return 1\n\
         }\n\
         cmd=\"$1\"\n\
         shift\n\
         service=''\n\
         account=''\n\
         password=''\n\
         want_secret='false'\n\
         while [ \"$#\" -gt 0 ]; do\n\
           case \"$1\" in\n\
             -s)\n\
               shift\n\
               service=\"$1\"\n\
               ;;\n\
             -a)\n\
               shift\n\
               account=\"$1\"\n\
               ;;\n\
             -w)\n\
               if [ \"$cmd\" = \"find-generic-password\" ]; then\n\
                 want_secret='true'\n\
               else\n\
                 shift\n\
                 if [ \"$#\" -gt 0 ] && [ \"${1#-}\" = \"$1\" ]; then\n\
                   password=\"$1\"\n\
                 else\n\
                   IFS= read -r password || true\n\
                   continue\n\
                 fi\n\
               fi\n\
               ;;\n\
           esac\n\
           shift\n\
         done\n\
         case \"$cmd\" in\n\
           find-generic-password)\n\
             if [ -n \"$account\" ]; then\n\
               item=\"$(item_dir \"$service\" \"$account\")\"\n\
             else\n\
               item=\"$(first_item_dir \"$service\")\" || item=''\n\
             fi\n\
             if [ -z \"$item\" ] || [ ! -f \"$item/secret\" ]; then\n\
               echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
               exit 44\n\
             fi\n\
             if [ \"$want_secret\" = 'true' ]; then\n\
               /bin/cat \"$item/secret\"\n\
             else\n\
               acct=$(/bin/cat \"$item/account\")\n\
               printf 'keychain: \"/fake/login.keychain-db\"\\n'\n\
               printf 'attributes:\\n'\n\
               printf '    \"acct\"<blob>=\"%s\"\\n' \"$acct\"\n\
             fi\n\
             ;;\n\
           add-generic-password)\n\
             item=\"$(item_dir \"$service\" \"$account\")\"\n\
             /bin/mkdir -p \"$item\"\n\
             printf '%s' \"$account\" > \"$item/account\"\n\
             printf '%s' \"$password\" > \"$item/secret\"\n\
             ;;\n\
           delete-generic-password)\n\
             item=\"$(item_dir \"$service\" \"$account\")\"\n\
             if [ -d \"$item\" ]; then\n\
               /bin/rm -rf \"$item\"\n\
             else\n\
                echo 'security: SecKeychainSearchCopyNext: The specified item could not be found in the keychain.' >&2\n\
                exit 44\n\
             fi\n\
             ;;\n\
           *)\n\
             echo \"unexpected security command: $cmd\" >&2\n\
             exit 1\n\
             ;;\n\
         esac\n",
    );
}

fn add_fake_tool_versions(env: &TestEnv) {
    env.add_fake_tool("claude", "2.1.87 (Claude Code)");
    env.add_fake_tool("codex", "codex-cli 0.117.0");
}

fn keychain_secret_path(env: &TestEnv, service: &str, account: &str) -> PathBuf {
    env.fake_home
        .join("keychain")
        .join(service)
        .join(account)
        .join("secret")
}

fn seed_keychain_item(env: &TestEnv, service: &str, account: &str, secret: &str) {
    let secret_path = keychain_secret_path(env, service, account);
    fs::create_dir_all(secret_path.parent().unwrap()).unwrap();
    fs::write(secret_path.parent().unwrap().join("account"), account).unwrap();
    fs::write(secret_path, secret).unwrap();
}

fn cmd_with_secure_env(env: &TestEnv) -> Command {
    let mut cmd = env.cmd();
    cmd.env("AISW_SECURITY_BIN", env.bin_dir.join("security"))
        .env("USER", "tester");
    cmd
}

fn secure_cmd_for_tool(env: &TestEnv, tool: &str) -> Command {
    let mut cmd = cmd_with_secure_env(env);
    match tool {
        "claude" => {
            cmd.env("AISW_CLAUDE_AUTH_STORAGE", "keychain");
        }
        "codex" => {
            cmd.env("AISW_CODEX_AUTH_STORAGE", "keychain");
        }
        _ => unreachable!(),
    }
    cmd
}

fn backup_id_for(env: &TestEnv, tool: &str, profile: &str) -> String {
    let output = cmd_with_secure_env(env)
        .args(["backup", "list", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let entries: serde_json::Value = serde_json::from_slice(&output).unwrap();
    entries
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["tool"] == tool && entry["profile"] == profile)
        .and_then(|entry| entry["backup_id"].as_str())
        .expect("expected backup entry")
        .to_owned()
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
