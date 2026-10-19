import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { randomUUID } from 'node:crypto';
import test from 'node:test';
import assert from 'node:assert/strict';
const require=createRequire(new URL('../../apps/desktop/package.json',import.meta.url));
const {JSDOM}=require('jsdom');
const source=readFileSync(new URL('../../apps/desktop/src-tauri/src/browser.js',import.meta.url),'utf8');
function page(){
  const dom=new JSDOM('<body><button id="counter" aria-label="Increment">Increment</button><input id="name"><input type="password" id="secret"></body>',{url:'http://localhost:3000',runScripts:'outside-only'});
  dom.window.HTMLElement.prototype.getClientRects=()=>[{width:10,height:10}];
  Object.defineProperty(dom.window.crypto,'randomUUID',{value:randomUUID,configurable:true});
  dom.window.__answers=[];
  dom.window.document.querySelector('#counter').onclick=event=>event.target.textContent='Clicked';
  const invoke=async(operation,args={})=>{
    const script=source.replace('__REQUEST__',JSON.stringify({operation,arguments:args,token:randomUUID()}))
      .replace(/const answer = value => \{ location.href = .*?; \};/, 'const answer = value => window.__answers.push(value);');
    await dom.window.eval(script);return dom.window.__answers.at(-1);
  };
  return {dom,invoke};
}
test('snapshot references click the same live DOM element',async()=>{
  const {dom,invoke}=page();try{
    const snapshot=await invoke('snapshot');const button=snapshot.elements.find(el=>el.name==='Increment');assert.ok(button?.ref);
    assert.equal((await invoke('click',{ref:button.ref})).acted,true);
    assert.equal(dom.window.document.querySelector('#counter').textContent,'Clicked');
  }finally{dom.window.close();}
});
test('a newer snapshot invalidates old references and password fields stay manual',async()=>{
  const {dom,invoke}=page();try{
    const snapshot=await invoke('snapshot');const input=snapshot.elements.find(el=>el.type==='password');
    assert.match((await invoke('fill',{ref:input.ref,text:'never write'})).error,/direct user input/);
    assert.equal(dom.window.document.querySelector('#secret').value,'');
    await invoke('snapshot');assert.match((await invoke('click',{ref:snapshot.elements[0].ref})).error,/Stale/);
  }finally{dom.window.close();}
});

test('snapshot works without a secure-context randomUUID API',async()=>{
  const {dom,invoke}=page();try{
    Object.defineProperty(dom.window.crypto,'randomUUID',{value:undefined});
    const snapshot=await invoke('snapshot');assert.ok(snapshot.elements?.length);
  }finally{dom.window.close();}
});

test('Tab moves focus and Enter in a textarea does not submit its form',async()=>{
  const {dom,invoke}=page();try{
    dom.window.document.body.innerHTML='<form><input id="first"><textarea id="text"></textarea><button>Send</button></form>';
    const first=dom.window.document.querySelector('#first');const textarea=dom.window.document.querySelector('#text');
    let submits=0;dom.window.document.querySelector('form').requestSubmit=()=>submits++;
    first.focus();assert.equal((await invoke('key',{key:'Tab'})).acted,true);
    assert.equal(dom.window.document.activeElement,textarea);
    textarea.value='ab';textarea.setSelectionRange(1,1);
    assert.equal((await invoke('key',{key:'Enter'})).acted,true);
    assert.equal(textarea.value,'a\nb');assert.equal(submits,0);
  }finally{dom.window.close();}
});

test('preventDefault stops Enter default action',async()=>{
  const {dom,invoke}=page();try{
    dom.window.document.body.innerHTML='<form><input></form>';
    const input=dom.window.document.querySelector('input');input.focus();
    let submits=0;input.form.requestSubmit=()=>submits++;
    input.addEventListener('keydown',event=>event.preventDefault());
    await invoke('key',{key:'Enter'});assert.equal(submits,0);
  }finally{dom.window.close();}
});
