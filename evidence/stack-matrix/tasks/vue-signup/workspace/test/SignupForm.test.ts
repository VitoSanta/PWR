import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/vue';
import userEvent from '@testing-library/user-event';
import SignupForm from '../src/SignupForm.vue';

afterEach(cleanup);

function setup(submit = vi.fn().mockResolvedValue(undefined)) {
  render(SignupForm, { props: { submit } });
  return { user: userEvent.setup(), submit };
}

async function fillValid(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText('Email'), '  ada@example.com ');
  await user.type(screen.getByLabelText('Password'), 'lovelace1');
  await user.type(screen.getByLabelText('Confirm password'), 'lovelace1');
  await user.click(screen.getByLabelText('I accept the terms'));
}

describe('SignupForm', () => {
  it('shows nothing before the fields are touched', () => {
    setup();
    expect(screen.queryAllByRole('alert')).toHaveLength(0);
    expect(screen.getByLabelText('Email').getAttribute('aria-invalid')).toBeNull();
  });

  it('shows a field error on blur, tied to the input, and follows the value live', async () => {
    const { user } = setup();
    const email = screen.getByLabelText('Email');
    await user.type(email, 'nope');
    await user.tab();
    const alert = screen.getByRole('alert');
    expect(alert.textContent).toBe('Enter a valid email');
    expect(email.getAttribute('aria-invalid')).toBe('true');
    expect(email.getAttribute('aria-describedby')).toBe(alert.id);
    expect(alert.id).not.toBe('');
    await user.type(email, '@x.io');
    expect(screen.queryByText('Enter a valid email')).toBeNull();
    expect(email.getAttribute('aria-invalid')).toBeNull();
    expect(email.getAttribute('aria-describedby')).toBeNull();
  });

  it('submitting with invalid fields shows every error and does not submit', async () => {
    const { user, submit } = setup();
    await user.type(screen.getByLabelText('Password'), 'short');
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    const messages = screen.getAllByRole('alert').map((a) => a.textContent);
    expect(messages).toEqual([
      'Enter a valid email',
      'At least 8 characters, with a letter and a digit',
      'Passwords do not match',
      'Accept the terms to continue',
    ]);
    expect(submit).not.toHaveBeenCalled();
  });

  it('submits the trimmed email and shows the welcome', async () => {
    const { user, submit } = setup();
    await fillValid(user);
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    expect(submit).toHaveBeenCalledTimes(1);
    expect(submit).toHaveBeenCalledWith({ email: 'ada@example.com', password: 'lovelace1' });
    expect((await screen.findByRole('status')).textContent).toBe('Welcome, ada@example.com!');
    expect(screen.queryByLabelText('Email')).toBeNull();
  });

  it('disables the button while pending and shows a server error', async () => {
    let fail!: (e: Error) => void;
    const submit = vi.fn(() => new Promise<void>((_, reject) => { fail = reject; }));
    const { user } = setup(submit);
    await fillValid(user);
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    const pending = screen.getByRole('button', { name: 'Signing up…' });
    expect(pending).toHaveProperty('disabled', true);
    fail(new Error('Email already registered'));
    expect((await screen.findByRole('alert')).textContent).toBe('Email already registered');
    expect(screen.getByRole('button', { name: 'Sign up' })).toHaveProperty('disabled', false);
    expect((screen.getByLabelText('Email') as HTMLInputElement).value.trim()).toBe('ada@example.com');
    expect((screen.getByLabelText('Password') as HTMLInputElement).value).toBe('lovelace1');
  });
});
