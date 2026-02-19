// Copyright 2026 Cloudflare, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use log::{debug, error};
use nix::unistd::{self, Group, User};
use std::ffi::CString;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use crate::server::configuration::ServerConf;

// Utilities to daemonize a pingora server, i.e. run the process in the background, possibly
// under a different running user and/or group.

// XXX: this operation should have been done when the old service is exiting.
// Now the new pid file just kick the old one out of the way
fn move_old_pid(path: &str) {
    if !Path::new(path).exists() {
        debug!("Old pid file does not exist");
        return;
    }
    let new_path = format!("{path}.old");
    match fs::rename(path, &new_path) {
        Ok(()) => {
            debug!("Old pid file renamed");
        }
        Err(e) => {
            error!(
                "failed to rename pid file from {} to {}: {}",
                path, new_path, e
            );
        }
    }
}

fn open_error_log(path: &str) -> std::fs::File {
    OpenOptions::new()
        .append(true)
        .create(true)
        // open read() in case there are no readers
        // available otherwise we will panic with
        // an ENXIO since O_NONBLOCK is set
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .unwrap_or_else(|e| panic!("failed to open error log file {path}: {e}"))
}

fn write_pid_file(path: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o640)
        .open(path)
        .unwrap_or_else(|e| panic!("failed to create pid file {path}: {e}"));
    writeln!(file, "{}", std::process::id())
        .unwrap_or_else(|e| panic!("failed to write pid file {path}: {e}"));
}

fn lookup_user(name: &str) -> User {
    User::from_name(name)
        .unwrap_or_else(|e| panic!("failed to look up user {name}: {e}"))
        .unwrap_or_else(|| panic!("user {name} not found"))
}

fn lookup_group(name: &str) -> nix::unistd::Gid {
    Group::from_name(name)
        .unwrap_or_else(|e| panic!("failed to look up group {name}: {e}"))
        .unwrap_or_else(|| panic!("group {name} not found"))
        .gid
}

fn init_supplementary_groups(user: &User) {
    let user_cstr = CString::new(user.name.as_str())
        .unwrap_or_else(|e| panic!("failed to create cstring for user {}: {e}", user.name));
    let base_group: libc::c_int = user
        .gid
        .as_raw()
        .try_into()
        .unwrap_or_else(|_| panic!("gid {} does not fit libc::c_int", user.gid.as_raw()));
    let ret = unsafe {
        libc::initgroups(
            user_cstr.as_ptr() as *const libc::c_char,
            base_group,
        )
    };
    if ret != 0 {
        panic!(
            "failed to initialize supplementary groups for user {}: {}",
            user.name,
            std::io::Error::last_os_error()
        );
    }
}

fn daemonize_process() {
    unsafe {
        let pid = libc::fork();
        if pid < 0 {
            panic!(
                "failed to fork during daemonization: {}",
                std::io::Error::last_os_error()
            );
        }
        if pid > 0 {
            libc::_exit(0);
        }

        if libc::setsid() < 0 {
            panic!(
                "failed to create a new session during daemonization: {}",
                std::io::Error::last_os_error()
            );
        }

        let pid = libc::fork();
        if pid < 0 {
            panic!(
                "failed to fork second stage during daemonization: {}",
                std::io::Error::last_os_error()
            );
        }
        if pid > 0 {
            libc::_exit(0);
        }
    }
}

/// Start a server instance as a daemon.
#[cfg(unix)]
pub fn daemonize(conf: &ServerConf) {
    // TODO: customize working dir
    unsafe {
        libc::umask(0o007);
    }

    let error_log = conf.error_log.as_deref().map(open_error_log);
    move_old_pid(&conf.pid_file);
    daemonize_process();

    if let Some(err) = error_log.as_ref() {
        let ret = unsafe { libc::dup2(err.as_raw_fd(), libc::STDERR_FILENO) };
        if ret < 0 {
            panic!(
                "failed to redirect stderr to configured error log: {}",
                std::io::Error::last_os_error()
            );
        }
    }

    let user = conf.user.as_deref().map(lookup_user);
    let mut target_gid = user.as_ref().map(|u| u.gid);
    if let Some(group_name) = conf.group.as_deref() {
        target_gid = Some(lookup_group(group_name));
    }

    write_pid_file(&conf.pid_file);
    if user.is_some() || target_gid.is_some() {
        let pid_file_gid = target_gid.clone();
        unistd::chown(
            Path::new(&conf.pid_file),
            user.as_ref().map(|u| u.uid),
            pid_file_gid,
        )
        .unwrap_or_else(|e| panic!("failed to change ownership of pid file {}: {e}", conf.pid_file));
    }

    if let Some(user) = user.as_ref() {
        init_supplementary_groups(user);
    }

    if let Some(gid) = target_gid {
        unistd::setgid(gid).unwrap_or_else(|e| panic!("failed to setgid to {gid}: {e}"));
    }

    if let Some(user) = user {
        unistd::setuid(user.uid).unwrap_or_else(|e| panic!("failed to setuid to {}: {e}", user.uid));
    }
}
