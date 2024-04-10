# Igor

The Igor vendoring tool offers generic vendoring of common fragments of text files that allow comments.

This tool is named after a character that appears in several Diskworld novels (my favorite is "Thief of Time"). This character offers a nice metaphor for what this tool tries to provide.
An Igor is an assistant who fills a niche. He uses time-tried solutions (like a good jolt of electricity obtained from a lightning bolt) to make his masters project come alive. There are many Igors, but they are quite interchangeable, so they all use the same name.
Every Igor has had (and has executed) many surgeries, has many visible scars and talks with a lisp. He isn't pretty, but he does the job.

This vendoring tool is similar. It offers inversion-of-control in vendoring dependencies. What is that supposed to mean, you might ask. Well, in a nutshell it works like this:

1. A project declares the niches it wants to be served by an Igor in a directory named `yeth-mathtur` (the strange spelling is caused by the lisp). For each niche there is a subdirectory with a name that matches the name of the niche that contains at least a file named `igor-thettingth.yaml` that specifies the name of the thundercloud project that provides lightning for this niche. File `igor-thettingth.yaml` can also be used to turn features on and off and otherwise change the process. The niche directory may contain additional files that complement or override files and fragments that are injected by Igor.

2. Igor watches thundercloud projects that provide lightning: files and fragments of files that can be injected into projects of mathturs.

3. When files change is thundercloud projects, Igor updates all the projects that declare the corresponding niche (unless opted out in `yeth-mathtur/nicheName/igor-thettingth.yaml`). If the settings file declares a build command, that is also run after the bolt of lightning hit the niche.

It is also possible to have Igor apply selected thundercloud projects to a mathturth' project.

Filenames in both the thundercloud projects and in the `yeth-mathtur/nicheName` directories of mathturth' projects are qualified with an infix before the last dot to denote their function.

* Option: `basename+option-featureName.ext` generates a file `basename.ext` only if the feature is turned on in the settings file
* Example: `basename+example-featureName.ext` generates a file `basename.ext` only if the feature is turned on in the settings file and the file does not exist
* Overwrite: `basename+overwrite-featureName.ext` signifies that `basename+example-featureName.ext` behaves like an option
* Unnamed fragment: `basename+fragment-featureName.ext` replaces placeholders with the ID `featureName` in `basename.ext` only if the feature is turned on in the settings file
* Named fragment: `basename+fragment-featureName-placeholderName.ext` replaces placeholders with the ID `featureName-placeholderName` in `basename.ext` only if the feature is turned on in the settings file
* Ignore: `basename+ignore-featureName.ext` ignores the instructions for file `basename.ext` in this niche only if the feature is turned on in the settings file (Ignore files are mainly useful in invar)

If the basename starts with `dot_`, then this prefix is replaced with a dot (`.`). If the basename starts with `x_`, then this prefix is removed. See the examples below.

If the basename is empty, then de hyphen that separates the basename from the infix may be omitted (see the example for `.bashrc` below).

A placeholder is either:

* One line that contains the substring `==== PLACEHOLDER placeholderId ====`

or a block of lines that is delimited by:

* One line that contains the substring `==== BEGIN placeholderId ====`
* One line that contains the substring `==== END placeholderId ====`

The replacement of a placeholder is always a placeholder with the same ID.

Special feature `@` is implicitly selected and cannot be turned off.

Names like featureName and placeholderName must begin with an alphabetic character or an underscore and may only contain alphabetic characters, underscores and numerical digits.

## Examples

Examples of lightning files:

* `api-def+option-protobuf.proto` generates `api-def.proto` if feature `proto` is selected
* `dot_bashrc+option-bash_config` generates `.bashrc` if feature `bash_config` is selected
* `x_dot_slash+option-contrived.md` generates `dot_slash.md` if feature `contrived` is selected
* `x_x_x+option-contrived` generates `x_x` if feature `contrived` is selected
* `Cargo-fragment+tokio-build_deps.toml` replaces placeholder `build_deps` in `Cargo.toml` if feature `tokio` is selected
* `main-ignore+niche.rs` ignores all lightning instructions from this niche for `main.rs`

Minimal settings file `yeth-mathtur/async-rust/igor-thettingth.yaml`:
```yaml
thundercloud:
  directory: "{{WORKAREA}}/async-rust-igor-thundercloud"
```

Elaborate settings file `yeth-mathtur/dendrite/igor-thettingth.yaml`
```yaml
%YAML 1.2
---
type:
  name: igor
  version: v0.1.0
thundercloud:
  git:
    remote: "git@github.com:rustigaan/dendrite-igor-thundercloud.git"
    revision: mathtur
    on-incoming: warn # update | ignore | warn | fail
options:
  selected:
    - mongodb # For query models that store data in MongoDB
    - grpc_ui # For an extra container that provides a web User Interface to call the gRPC backend
  deselected:
    - frontend
settings:
  watch: false
```
