// Executed only in the selected remote webview. No Tauri APIs are exposed.
(async () => {
  const {operation, arguments: args, token} = __REQUEST__;
  const answer = value => { location.href = 'pwr-browser://reply/' + token + '?result=' + encodeURIComponent(JSON.stringify(value)); };
  try {
    const visible = el => !!(el.getClientRects().length && getComputedStyle(el).visibility !== 'hidden');
    const snapshot = () => {
      const nonce = token;
      const refs = new Map();
      const elements = [...document.querySelectorAll('a[href],button,input,textarea,select,[role="button"],[role="link"],[contenteditable="true"]')].filter(visible).slice(0, 300).map((el, i) => {
        const ref = nonce + ':' + i; refs.set(ref, el);
        return {ref, tag: el.tagName.toLowerCase(), role: el.getAttribute('role'), name: (el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.innerText || el.getAttribute('name') || '').slice(0, 180), type: el.getAttribute('type'), disabled: !!el.disabled};
      });
      window.__pwrBrowserRefs = refs;
      return {url: location.href, title: document.title, text: (document.body?.innerText || '').slice(0, 20000), elements};
    };
    const element = () => {
      const el = window.__pwrBrowserRefs?.get(args.ref);
      if (!el || !el.isConnected || !visible(el)) throw new Error('Stale element reference. Take a new snapshot.');
      return el;
    };
    if (operation === 'snapshot') return answer(snapshot());
    if (operation === 'click') element().click();
    else if (operation === 'fill') {
      const el = element();
      if (el.disabled || el.readOnly || (!['INPUT','TEXTAREA','SELECT'].includes(el.tagName) && !el.isContentEditable) || ['file','password'].includes(el.type)) throw new Error('This field requires direct user input or cannot be filled.');
      const text = String(args.text || '').slice(0, 16000);
      if (el.tagName === 'SELECT') {
        const option = [...el.options].find(o => o.value === text || o.text.trim() === text.trim());
        if (!option) throw new Error('No matching option in select.');
        el.value = option.value;
      } else if (el.isContentEditable) {
        el.focus();
        el.textContent = text;
      } else {
        const prototype = el.tagName === 'INPUT' ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
        Object.getOwnPropertyDescriptor(prototype, 'value').set.call(el, text);
      }
      el.dispatchEvent(new Event('input', {bubbles:true})); el.dispatchEvent(new Event('change', {bubbles:true}));
    } else if (operation === 'key') {
      const el = args.ref ? element() : document.activeElement;
      if (!['Enter','Escape','Tab','ArrowDown','ArrowUp','ArrowLeft','ArrowRight',' '].includes(args.key)) throw new Error('Unsupported key.');
      el?.focus?.();
      const proceed = el?.dispatchEvent(new KeyboardEvent('keydown',{key:args.key,bubbles:true,cancelable:true}));
      if (proceed && args.key === 'Tab') {
        const targets = [...document.querySelectorAll('a[href],button,input,textarea,select,[tabindex],[contenteditable="true"]')]
          .filter(target => visible(target) && !target.disabled && target.tabIndex >= 0 && !target.closest('[inert]'))
          .sort((left,right) => (left.tabIndex || Number.MAX_SAFE_INTEGER) - (right.tabIndex || Number.MAX_SAFE_INTEGER));
        if (!targets.length) throw new Error('The page has no focusable elements.');
        targets[(targets.indexOf(el) + 1) % targets.length].focus();
      } else if (proceed && args.key === 'Enter') {
        if (el?.tagName === 'TEXTAREA') {
          if (el.disabled || el.readOnly) throw new Error('This field cannot be edited.');
          if (el.dispatchEvent(new InputEvent('beforeinput',{bubbles:true,cancelable:true,inputType:'insertLineBreak',data:null}))) {
            el.setRangeText('\n',el.selectionStart,el.selectionEnd,'end');
            el.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertLineBreak',data:null}));
          }
        } else if (el?.matches('button,a[href],input[type="submit"],input[type="button"]')) el.click();
        else if (el?.tagName === 'INPUT' && el.form) el.form.requestSubmit();
      }
      (document.activeElement || el)?.dispatchEvent(new KeyboardEvent('keyup',{key:args.key,bubbles:true,cancelable:true}));
    } else if (operation === 'scroll') window.scrollBy(0, Math.max(-2000,Math.min(2000,Number(args.delta)||600)));
    else if (operation === 'back') history.back();
    else if (operation === 'forward') history.forward();
    else if (operation === 'reload') location.reload();
    else throw new Error('Unsupported browser operation.');
    // A real navigation can destroy this context. Its outcome is then uncertain,
    // never silently retried: the next call must observe the destination.
    answer({acted:true, url:location.href, title:document.title});
  } catch (error) { answer({error:String(error)}); }
})();
