# Repository rules

## Language

English is the primary language of this repository. Write everything in English: code comments, log and error messages (startup errors included), UI copy, documentation, commit messages, and pull request text. The only Chinese documentation is `README.zh-CN.md`, a companion Simplified Chinese translation of `README.md`; update it whenever `README.md` changes.

Do not add new Chinese text outside these three exceptions: `README.zh-CN.md`, as described above; test data that exercises non-ASCII handling, such as Chinese object names; and the Simplified Chinese UI locale catalog that spec 6.7 requires. Other Chinese that still exists is pending translation.

## CI

GitHub Actions builds and publishes official releases from validated version tags. Keep the
release workflow enabled. Local builds are for development verification only; official release
images and release assets must be produced by GitHub Actions.
