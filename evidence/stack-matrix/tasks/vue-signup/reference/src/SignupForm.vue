<script setup lang="ts">
import { computed, reactive, ref, useId } from 'vue';

const props = defineProps<{ submit: (data: { email: string; password: string }) => Promise<void> }>();

type Field = 'email' | 'password' | 'confirm' | 'terms';

const values = reactive({ email: '', password: '', confirm: '', terms: false });
const touched = reactive<Record<Field, boolean>>({ email: false, password: false, confirm: false, terms: false });
const pending = ref(false);
const serverError = ref('');
const welcomed = ref('');
const base = useId();

const problems = computed<Record<Field, string>>(() => ({
  email: /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(values.email.trim()) ? '' : 'Enter a valid email',
  password:
    values.password.length >= 8 && /[A-Za-z]/.test(values.password) && /\d/.test(values.password)
      ? ''
      : 'At least 8 characters, with a letter and a digit',
  confirm: values.confirm === values.password ? '' : 'Passwords do not match',
  terms: values.terms ? '' : 'Accept the terms to continue',
}));

const shown = (field: Field) => (touched[field] ? problems.value[field] : '');
const errorId = (field: Field) => `${base}-${field}-error`;
const described = (field: Field) =>
  shown(field) ? { 'aria-invalid': 'true', 'aria-describedby': errorId(field) } : {};

async function onSubmit() {
  (Object.keys(touched) as Field[]).forEach((field) => (touched[field] = true));
  serverError.value = '';
  if (Object.values(problems.value).some(Boolean)) return;
  pending.value = true;
  try {
    const email = values.email.trim();
    await props.submit({ email, password: values.password });
    welcomed.value = email;
  } catch (error) {
    serverError.value = error instanceof Error ? error.message : String(error);
  } finally {
    pending.value = false;
  }
}
</script>

<template>
  <p v-if="welcomed" role="status">Welcome, {{ welcomed }}!</p>
  <form v-else novalidate @submit.prevent="onSubmit">
    <p v-if="serverError" role="alert">{{ serverError }}</p>
    <div>
      <label :for="`${base}-email`">Email</label>
      <input :id="`${base}-email`" v-model="values.email" type="email" v-bind="described('email')" @blur="touched.email = true" />
      <p v-if="shown('email')" :id="errorId('email')" role="alert">{{ shown('email') }}</p>
    </div>
    <div>
      <label :for="`${base}-password`">Password</label>
      <input :id="`${base}-password`" v-model="values.password" type="password" v-bind="described('password')" @blur="touched.password = true" />
      <p v-if="shown('password')" :id="errorId('password')" role="alert">{{ shown('password') }}</p>
    </div>
    <div>
      <label :for="`${base}-confirm`">Confirm password</label>
      <input :id="`${base}-confirm`" v-model="values.confirm" type="password" v-bind="described('confirm')" @blur="touched.confirm = true" />
      <p v-if="shown('confirm')" :id="errorId('confirm')" role="alert">{{ shown('confirm') }}</p>
    </div>
    <div>
      <input :id="`${base}-terms`" v-model="values.terms" type="checkbox" v-bind="described('terms')" @blur="touched.terms = true" />
      <label :for="`${base}-terms`">I accept the terms</label>
      <p v-if="shown('terms')" :id="errorId('terms')" role="alert">{{ shown('terms') }}</p>
    </div>
    <button type="submit" :disabled="pending">{{ pending ? 'Signing up…' : 'Sign up' }}</button>
  </form>
</template>
