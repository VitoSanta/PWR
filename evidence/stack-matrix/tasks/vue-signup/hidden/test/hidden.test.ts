import { describe, expect, it, vi, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/vue';
import userEvent from '@testing-library/user-event';
import SignupForm from '../src/SignupForm.vue';

afterEach(cleanup);

describe('SignupForm (hidden)', () => {
  it('confirm error follows changes to the password too', async () => {
    const user = userEvent.setup();
    render(SignupForm, { props: { submit: vi.fn() } });
    await user.type(screen.getByLabelText('Password'), 'abcdefg1');
    await user.type(screen.getByLabelText('Confirm password'), 'abcdefg1');
    await user.tab();
    expect(screen.queryAllByRole('alert')).toHaveLength(0);
    await user.type(screen.getByLabelText('Password'), '2');
    expect(screen.getAllByRole('alert').map((a) => a.textContent)).toContain('Passwords do not match');
  });

  it('a password of letters only or digits only is invalid', async () => {
    const user = userEvent.setup();
    render(SignupForm, { props: { submit: vi.fn() } });
    const password = screen.getByLabelText('Password');
    const message = 'At least 8 characters, with a letter and a digit';
    await user.type(password, 'abcdefgh');
    await user.tab();
    expect(screen.getByText(message).getAttribute('role')).toBe('alert');
    await user.clear(password);
    await user.type(password, '12345678');
    expect(screen.getByText(message).getAttribute('role')).toBe('alert');
    await user.type(password, 'x');
    expect(screen.queryByText(message)).toBeNull();
    expect(password.getAttribute('aria-invalid')).toBeNull();
  });

  it('unticking the terms after a submit attempt brings the error back', async () => {
    const user = userEvent.setup();
    render(SignupForm, { props: { submit: vi.fn() } });
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    const terms = screen.getByLabelText('I accept the terms');
    await user.click(terms);
    expect(screen.getAllByRole('alert').map((a) => a.textContent)).not.toContain('Accept the terms to continue');
    await user.click(terms);
    expect(screen.getAllByRole('alert').map((a) => a.textContent)).toContain('Accept the terms to continue');
    expect(terms.getAttribute('aria-invalid')).toBe('true');
  });

  it('the server error goes away on the next submit and ids are unique', async () => {
    const user = userEvent.setup();
    const submit = vi.fn().mockRejectedValueOnce(new Error('Try later')).mockResolvedValueOnce(undefined);
    render(SignupForm, { props: { submit } });
    await user.type(screen.getByLabelText('Email'), 'a@b.co');
    await user.type(screen.getByLabelText('Password'), 'abcdefg1');
    await user.type(screen.getByLabelText('Confirm password'), 'abcdefg1');
    await user.click(screen.getByLabelText('I accept the terms'));
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    expect((await screen.findByRole('alert')).textContent).toBe('Try later');
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    expect((await screen.findByRole('status')).textContent).toBe('Welcome, a@b.co!');
    expect(screen.queryAllByRole('alert')).toHaveLength(0);
  });

  it('every field error has its own id', async () => {
    const user = userEvent.setup();
    render(SignupForm, { props: { submit: vi.fn() } });
    await user.click(screen.getByRole('button', { name: 'Sign up' }));
    await user.type(screen.getByLabelText('Password'), 'x');
    const ids = screen.getAllByRole('alert').map((a) => a.id);
    expect(ids).toHaveLength(4);
    expect(new Set(ids).size).toBe(4);
  });
});
