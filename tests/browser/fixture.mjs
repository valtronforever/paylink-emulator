import {test as base,expect} from '@playwright/test';
import {spawn,execFileSync} from 'node:child_process';
import {createServer} from 'node:https';
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {randomUUID} from 'node:crypto';
const device='00000000-0000-4000-8000-000000000001';
export {expect,device};
export const instant={timing:{connect_ms:0,card_ms:0,customer_ms:0,authorize_ms:0,confirm_ms:0,response_ms:0,timeout_ms:120000}};
export const test=base.extend({
  emulator: async ({page},use,testInfo)=>{
    const dir=testInfo.outputPath('runtime');mkdirSync(dir,{recursive:true});
    const key=join(dir,'key.pem'),cert=join(dir,'cert.pem');
    execFileSync('openssl',['req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',cert,'-days','1','-subj','/CN=localhost'],{stdio:'ignore'});
    let paymentUrl='';
    const harness=createServer({key:readFileSync(key),cert:readFileSync(cert)},(_req,res)=>{
      res.setHeader('Content-Type','text/html');
      res.end(`<!doctype html><html lang="en"><title>PayLink transport harness</title><body><h1>Card payment</h1><button id="pay">Pay 1.00 UAH</button><output id="result">Ready</output><script>
      window.endpoint=${JSON.stringify(paymentUrl)};
      document.querySelector('#pay').onclick=async()=>{
        const result=document.querySelector('#result');result.textContent='Processing';
        try{const r=await fetch(window.endpoint+'/api/pos/${device}/purchase',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({amount:100})});const data=await r.json();result.textContent=data.success?'Approved':data.error;}
        catch(e){result.textContent='Transport error';}
      };
      </script></body></html>`);
    });
    await new Promise(r=>harness.listen(0,'127.0.0.1',r));
    const origin=`https://127.0.0.1:${harness.address().port}`;
    const token=randomUUID();
    const bin=process.env.PAYLINK_BIN || resolve(`target/debug/paylink-emulator${process.platform==='win32'?'.exe':''}`);
    const child=spawn(bin,['serve','--payment-addr','127.0.0.1:0','--control-addr','127.0.0.1:0','--allow-origin',origin,'--journal',join(dir,'journal.json')],{env:{...process.env,PAYLINK_CONTROL_TOKEN:token},stdio:['ignore','pipe','pipe']});
    let stderr='';child.stderr.on('data',b=>stderr+=b);
    const ready=await new Promise((resolve,reject)=>{
      let output='';const timer=setTimeout(()=>reject(new Error(`Readiness timed out: ${stderr}`)),10000);
      child.on('error',e=>{clearTimeout(timer);reject(e)});
      child.on('exit',code=>{clearTimeout(timer);reject(new Error(`Server exited ${code}: ${stderr}`))});
      child.stdout.on('data',chunk=>{output+=chunk;if(output.includes('\n')){clearTimeout(timer);resolve(JSON.parse(output.split('\n')[0]))}});
    });
    paymentUrl=ready.payment_url;
    const control=async(resource,payload)=>{
      const response=await fetch(`${ready.control_url}/control/v1/${resource}`,{
        method:payload===undefined?'GET':'POST',headers:{Authorization:`Bearer ${token}`,'Content-Type':'application/json'},
        ...(payload===undefined?{}:{body:JSON.stringify({command_id:randomUUID(),payload})})
      });
      const value=await response.json();if(!response.ok)throw new Error(`${resource}: ${JSON.stringify(value)}`);return value;
    };
    try {
      await page.goto(origin);
      await use({control,origin,paymentUrl,arm:scenario=>control('arm',scenario)});
    }finally{
      try{writeFileSync(join(dir,'final-journal.json'),JSON.stringify(await control('journal'),null,2));}catch{}
      writeFileSync(join(dir,'stderr.log'),stderr);
      child.kill('SIGINT');await new Promise(r=>{if(child.exitCode!==null)return r();child.once('exit',r);setTimeout(()=>{child.kill('SIGKILL');r()},3000).unref()});
      await new Promise(r=>harness.close(r));
    }
  }
});
