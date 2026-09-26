# signup

The sign-up form of the web app: a Vue 3 single-file component,
`src/SignupForm.vue`, written with `<script setup lang="ts">`.

```vue
<SignupForm :submit="api.signUp" />
```

Prop `submit: (data: { email: string; password: string }) => Promise<void>`,
required.

## Fields

Each has a visible `<label>` tied to its input:

| Label | Input | Valid when | Error message |
|---|---|---|---|
| `Email` | `type="email"` | matches `^[^\s@]+@[^\s@]+\.[^\s@]+$` after trimming | `Enter a valid email` |
| `Password` | `type="password"` | at least 8 characters, with a letter and a digit | `At least 8 characters, with a letter and a digit` |
| `Confirm password` | `type="password"` | equals the password | `Passwords do not match` |
| `I accept the terms` | `type="checkbox"` | checked | `Accept the terms to continue` |

## Behaviour

- A field's error is shown once the field has been left (blur) or a submit
  was attempted, and from then on follows the field live (it disappears as
  soon as the value is valid, and comes back if it stops being). Before that
  nothing is shown.
- A shown error is an element with `role="alert"` holding the message; its
  input has `aria-invalid="true"` and `aria-describedby` set to the error's
  `id`. A field without a shown error has neither attribute.
- The button `Sign up` submits. Submitting with any invalid field shows every
  error and does not call `submit`. Otherwise `submit` is called once with
  the trimmed email and the password; while it is pending the button reads
  `Signing up…` and is disabled.
- If `submit` rejects, the error's `message` is shown in a form-level
  `role="alert"`, the fields keep their values and the button is enabled
  again. The next submit clears that message.
- If it resolves, the form is replaced by a `role="status"` element saying
  `Welcome, <email>!`.

Run the tests with `npm test` after `npm install`.
