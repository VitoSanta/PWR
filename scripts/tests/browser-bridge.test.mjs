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
  Object.defineProperty(dom.window.crypto,'randomUUID',{value:randomUUID});
  dom.window.__answers=[];
  dom.window.document.querySelector('#counter').onclick=event=>event.target.textContent='Clicked';
  const invoke=async(operation,args={})=>{
    const script=source.replace('__REQUEST__',JSON.stringify({operation,arguments:args,token:'test'}))
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
