import {test,expect,instant} from './fixture.mjs';
test('HTTPS page uses real PayLink HTTP listener and realistic delay',async({page,emulator})=>{
  await emulator.arm({...instant,timing:{...instant.timing,authorize_ms:250}});
  await page.getByRole('button',{name:'Pay 1.00 UAH'}).click();
  await expect(page.locator('#result')).toHaveText('Processing');
  await expect(page.locator('#result')).toHaveText('Approved');
  await emulator.control('assert',{approvals:1,accepted:1,queue_empty:true,idle:true});
  await page.screenshot({path:test.info().outputPath('approved.png')});
});
test('lost reply, reload and retry expose duplicate charge',async({page,emulator})=>{
  await emulator.arm({...instant,delivery:'disconnect_after_commit'});
  await page.getByRole('button').click();await expect(page.locator('#result')).toHaveText('Transport error');
  await emulator.control('assert',{approvals:1,delivered:0});
  await page.reload();await emulator.arm(instant);await page.getByRole('button').click();
  await expect(page.locator('#result')).toHaveText('Approved');
  await emulator.control('assert',{approvals:2,accepted:2,requests:2});
});
test('all documented terminal errors are visible and recoverable',async({page,emulator})=>{
  const errors=await emulator.control('errors');
  for(const error of errors.filter(e=>e.category==='terminal')){
    await emulator.arm({...instant,outcome:'error',error_id:error.id});
    await page.getByRole('button').click();await expect(page.locator('#result')).toHaveText(error.message);
    await emulator.arm(instant);await page.getByRole('button').click();await expect(page.locator('#result')).toHaveText('Approved');
  }
  await emulator.control('assert',{approvals:11,accepted:22,queue_empty:true});
});
test('manual card event is held until control action',async({page,emulator})=>{
  await emulator.arm({...instant,mode:'manual'});await page.getByRole('button').click();
  await expect(page.locator('#result')).toHaveText('Processing');
  let operation;
  await expect.poll(async()=>{const state=await emulator.control('state');operation=Object.values(state.operations)[0];return operation?.stage}).toBe('awaiting_card');
  await emulator.control('action',{operation_id:operation.id,event:'card_presented'});
  await emulator.control('action',{operation_id:operation.id,event:'customer_confirmed'});
  await expect(page.locator('#result')).toHaveText('Approved');
});
test('blocked origin fails actual browser CORS',async({page,emulator})=>{
  await emulator.arm(instant);
  // Different hostname is a different origin; the HTTPS harness serves both.
  await page.goto(emulator.origin.replace('127.0.0.1','localhost'));
  await page.getByRole('button').click();await expect(page.locator('#result')).toHaveText('Transport error');
  await emulator.control('assert',{requests:0,approvals:0});
});
