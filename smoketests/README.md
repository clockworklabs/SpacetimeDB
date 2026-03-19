# Python Smoketests (Legacy)

> **Note:** These Python smoketests are being replaced by Rust smoketests in `crates/smoketests/`.
> Both test suites currently run in CI to ensure consistency during the transition.
>
> For new tests, please add them to the Rust smoketests. See `crates/smoketests/DEVELOP.md` for instructions.

---

## Running the Python Smoketests

To use the smoketests, you first need to install the dependencies:

```
python -m venv smoketests/venv
smoketests/venv/bin/pip install -r smoketests/requirements.txt
```

Then, run the smoketests like so:
```
smoketests/venv/bin/python -m smoketests <args>
```
