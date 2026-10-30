process.env.CU_SERVER_TEST='1';
const {test}=require('node:test');
const assert=require('node:assert/strict');
const {EventEmitter}=require('node:events');
const {PassThrough}=require('node:stream');
const fs=require('node:fs');
const {createDashboard}=require('./server.cjs');
const html=fs.readFileSync(__dirname+'/index.html','utf8');
const tick=()=>new Promise(setImmediate);

test('private access, real-time approvals, run isolation, cancellation and recovery',async t=>{
  const children=[];
  const dashboard=createDashboard({binary:'fake-cu',html,token:'private-code',
    execute:async(file)=>file==='codex'?{stdout:'Logged in using ChatGPT',stderr:''}:{stdout:'{"screen_recording":true,"accessibility":true}',stderr:''},
    spawnRun:(file,args)=>{
      const child=new EventEmitter();child.stdout=new PassThrough();child.stderr=new PassThrough();child.stdin=new PassThrough();child.args=args;
      child.kill=signal=>{child.signal=signal;setImmediate(()=>child.emit('close',130));};
      children.push(child);return child;
    }});
  dashboard.server.listen(0,'127.0.0.1');await new Promise(resolve=>dashboard.server.once('listening',resolve));
  t.after(()=>dashboard.close());
  const base=`http://127.0.0.1:${dashboard.server.address().port}`;
  const request=(url,body,extra={})=>fetch(base+url,{...(body===undefined?{}:{method:'POST',body:JSON.stringify(body)}),headers:{'Content-Type':'application/json',...extra}});
  assert.equal((await request('/api/status')).status,401);
  const landing=await request('/');assert.equal(landing.status,200);assert.match(landing.headers.get('content-security-policy'),/sha256-/);
  assert.equal((await request('/api/connect',{token:'wrong'})).status,401);
  assert.equal((await request('/api/connect',{token:'private-code'},{Origin:'http://evil.example'})).status,403);
  const login=await request('/api/connect',{token:'private-code'});assert.equal(login.status,200);assert.match(login.headers.get('set-cookie'),/HttpOnly; SameSite=Strict/);
  const auth={Cookie:'cu_session=private-code'};
  assert.equal((await(await request('/api/status',undefined,auth)).json()).ready,true);
  assert.equal((await request('/api/run',{task:'hello',mode:'auto'},auth)).status,400);
  assert.equal((await request('/api/run',{task:' '.repeat(5),mode:'observe'},auth)).status,400);
  assert.equal((await request('/api/run',{task:'x'.repeat(17000),mode:'observe'},auth)).status,413);
  const task='show weather; $(echo injected)';
  const started=await request('/api/run',{task,mode:'confirm-risky'},auth);assert.equal(started.status,202);
  const run=(await started.json()).run;assert.ok(children[0].args.includes(task));
  assert.equal((await request('/api/run',{task:'second',mode:'observe'},auth)).status,409);
  children[0].stdout.write(JSON.stringify({type:'confirmation',id:1,kind:'action',message:'Allow sending?'})+'\n');await tick();
  const state=await(await request('/api/status',undefined,auth)).json();assert.equal(state.run.state,'waiting');assert.equal(state.run.pending.id,1);
  assert.equal((await request('/api/confirm',{runId:'old-run',id:1,approved:true},auth)).status,409);
  assert.equal((await request('/api/confirm',{runId:run.id,id:1,approved:true},auth)).status,200);
  assert.equal(JSON.parse(children[0].stdin.read().toString()).approved,true);
  assert.equal((await request('/api/confirm',{runId:run.id,id:1,approved:true},auth)).status,409);
  const stream=await request('/api/events',undefined,auth);assert.equal(stream.headers.get('content-type'),'text/event-stream');
  const reader=stream.body.getReader();const first=await reader.read();assert.match(new TextDecoder().decode(first.value),/connected/);await reader.cancel();
  assert.equal((await request('/api/stop',{},auth)).status,200);await tick();
  assert.equal(children[0].signal,'SIGINT');assert.equal(children[0].stdin.writableEnded,true);
  const stopped=await(await request('/api/status',undefined,auth)).json();assert.equal(stopped.run.state,'aborted');
  const again=await request('/api/run',{task:'new task',mode:'observe'},auth);assert.equal(again.status,202);
  children[1].stdout.write(JSON.stringify({type:'done',summary:{status:'done',steps:1,actions_executed:0,wall_ms:5,final_text:'Complete'}})+'\n');await tick();children[1].emit('close',0);
  const done=await(await request('/api/status',undefined,auth)).json();assert.equal(done.run.state,'done');assert.equal(done.run.summary.final_text,'Complete');
  const disconnect=await request('/api/disconnect',{},auth);assert.match(disconnect.headers.get('set-cookie'),/Max-Age=0/);
});
