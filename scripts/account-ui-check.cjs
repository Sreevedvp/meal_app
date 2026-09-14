// Run against MEAL_REQUIRE_AUTH=true assets. Session responses are mocked to test
// the browser boundary; PostgreSQL integration tests exercise server ownership.
const {chromium}=require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({headless:true,executablePath:process.env.CHROMIUM_PATH || undefined});
 try {
  const page=await browser.newPage();
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  let account='alice';
  await page.route('**/api/session',route=>account==='expired'
   ? route.fulfill({status:401,contentType:'application/json',body:'{"message":"Sign in"}'})
   : route.fulfill({contentType:'application/json',body:JSON.stringify({production:true,storage_scope:`user-test-${account}`})}));
  const ready=async()=>{
   await page.goto('http://127.0.0.1:8081');
   await page.getByRole('button',{name:'Sign out',exact:true}).waitFor();
   await page.waitForFunction(()=>!document.querySelector('fieldset.configuration').disabled);
  };
  await ready();
  await page.locator('#total-calories').fill('2400');
  await page.getByRole('button',{name:'Save my settings'}).click();
  await page.waitForFunction(()=>!!localStorage.getItem('user-test-alice:deccan-fitness-v1'));
  assert.equal(await page.evaluate(()=>localStorage.getItem('deccan-fitness-v1')),null);
  account='bob';await ready();
  assert.equal(await page.locator('#total-calories').inputValue(),'2100.0');
  await page.locator('#total-calories').fill('2200');
  await page.getByRole('button',{name:'Save my settings'}).click();
  await page.waitForFunction(()=>!!localStorage.getItem('user-test-bob:deccan-fitness-v1'));
  account='alice';await ready();
  assert.equal(await page.locator('#total-calories').inputValue(),'2400.0');
  await page.route('**/oauth2/sign_out**',route=>route.fulfill({contentType:'text/html',body:'<h1>Signed out</h1>'}));
  await page.getByRole('button',{name:'Sign out',exact:true}).click();
  await page.getByRole('heading',{name:'Signed out',exact:true}).waitFor();
  assert.equal(await page.evaluate(()=>localStorage.getItem('user-test-alice:deccan-fitness-v1')),null);
  assert.ok(await page.evaluate(()=>localStorage.getItem('user-test-bob:deccan-fitness-v1')));
  account='expired';await page.goto('http://127.0.0.1:8081');
  await page.getByRole('heading',{name:'Sign in to Deccan Table',exact:true}).waitFor();
  assert.equal(await page.locator('#total-calories').count(),0);
  assert.deepEqual(errors,[]);
  console.log('PASS: production session required; account-specific preferences; sign-out clears only the current account; expired session cannot open the planner. OIDC responses mocked.');
 } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exit(1);});
