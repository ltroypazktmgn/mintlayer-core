#!/usr/bin/env python3
# Some simple custom code lints, mostly implemented by means of grepping code

import os
import re
import sys
import toml

SCALECODEC_RE = r'\bparity_scale_codec(_derive)?::'
JSONRPSEE_RE = r'\bjsonrpsee[_a-z0-9]*::'

LICENSE_TEMPLATE = [
    r'// Copyright \(c\) 202[0-9](-202[0-9])? .+',
    r'// opensource@mintlayer\.org',
    r'// SPDX-License-Identifier: MIT',
    r'// Licensed under the MIT License;',
    r'// you may not use this file except in compliance with the License\.',
    r'// You may obtain a copy of the License at',
    r'//',
    r'// https://github.com/mintlayer/mintlayer-core/blob/master/LICENSE',
    r'//',
    r'// Unless required by applicable law or agreed to in writing, software',
    r'// distributed under the License is distributed on an "AS IS" BASIS,',
    r'// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied\.',
    r'// See the License for the specific language governing permissions and',
    r'// limitations under the License\.',
    r'|//'
]

# List Rust source files
def rs_sources(exclude = []):
    exclude = [ os.path.normpath(dir) for dir in ['target', '.git', '.github'] + exclude ]
    is_excluded = lambda top, d: os.path.normpath(os.path.join(top, d).lower()) in exclude

    for top, dirs, files in os.walk('.', topdown=True):
        dirs[:] = [ d for d in dirs if not is_excluded(top, d) ]
        for file in files:
            if os.path.splitext(file)[1].lower() == '.rs':
                yield os.path.join(top, file)

# Disallow certain pattern in source files, with exceptions
def disallow(pat, exclude = []):
    print("==== Searching for '{}':".format(pat))
    pat = re.compile(pat)

    found_re = False
    for path in rs_sources(exclude):
        with open(path) as file:
            for (line_num, line) in enumerate(file, start = 1):
                line = line.rstrip()
                if pat.search(line):
                    found_re = True
                    print("{}:{}:{}".format(path, line_num, line))

    print()
    return not found_re

# Check we depend on only one version of given crate
def check_crate_version_unique(crate_name):
    packages = toml.load('Cargo.lock')['package']
    versions = [ p['version'] for p in packages if p['name'] == crate_name ]

    if len(versions) == 0:
        print("Crate missing: '{}'".format(crate_name))
    if len(versions) >= 2:
        print("Multiple versions of '{}': {}".format(crate_name, ', '.join(versions)))

    return len(versions) == 1

# Ensure that the versions in the workspace's Cargo.toml are consistent
def check_workspace_and_package_versions_equal():
    print("==== Ensuring workspace and package versions are equal in the workspace's Cargo.toml")
    root = toml.load('Cargo.toml')

    workspace_version = root['package']['version']
    package_version = root['workspace']['package']['version']

    result = workspace_version == package_version

    if not result:
        print("Workspace vs package versions mismatch in Cargo.toml: '{}' != '{}'".format(workspace_version, package_version))
    print()

    return result


# Check crate versions
def check_crate_versions():
    print("==== Checking crate versions:")

    ok = all([
            check_crate_version_unique('parity-scale-codec'),
            check_crate_version_unique('parity-scale-codec-derive'),
        ])

    print()
    return ok

# Check license header in current project crates
def check_local_licenses():
    print("==== Checking local license headers:")

    # list of files exempted from license check
    exempted_files = [
        "./script/src/opcodes.rs",
        "./common/src/uint/internal_macros.rs",
        "./common/src/uint/endian.rs",
        "./common/src/uint/impls.rs",
        "./common/src/uint/mod.rs"
        ]

    template = re.compile('(?:' + r')\n(?:'.join(LICENSE_TEMPLATE) + ')')

    ok = True
    for path in rs_sources():
        if any(os.path.samefile(path, exempted) for exempted in exempted_files):
            continue

        with open(path) as file:
            if not template.search(file.read()):
                ok = False
                print("{}: License missing or incorrect".format(path))

    print()
    return ok

# check TODO(PR) and FIXME instances
def check_todos():
    print("==== Checking TODO(PR) and FIXME instances:")

    # list of files exempted from checks
    exempted_files = [
        ]

    ok = True
    for path in rs_sources():
        if any(os.path.samefile(path, exempted) for exempted in exempted_files):
            continue

        with open(path) as file:
            file_data = file.read()
            if 'TODO(PR)' in file_data or 'FIXME' in file_data or 'todo!()' in file_data:
                ok = False
                print("{}: Found TODO(PR) or FIXME or todo!() instances".format(path))

    print()
    return ok

def run_checks():
    return all([
            disallow(SCALECODEC_RE, exclude = ['serialization/core', 'merkletree']),
            disallow(JSONRPSEE_RE, exclude = ['rpc', 'wallet/wallet-node-client']),
            check_local_licenses(),
            check_crate_versions(),
            check_workspace_and_package_versions_equal(),
            check_todos()
        ])

if __name__ == '__main__':
    if not run_checks():
        sys.exit(1)
