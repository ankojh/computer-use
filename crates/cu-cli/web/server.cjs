// Dependency-free mobile bridge. The native cu process remains the only executor.
const http = require('node:http');
const crypto = require('node:crypto');
const os = require('node:os');
const fs = require('node:fs/promises');
const path = require('node:path');
const { spawn, execFile } = require('node:child_process');
const { promisify } = require('node:util');
const { createInterface } = require('node:readline');
const exec = promisify(execFile);

function createDashboard({ binary = process.env.CU_BINARY, html = process.env.CU_WEB_HTML,
  token = crypto.randomBytes(32).toString('hex'), config = process.env.CU_CONFIG,
  execute = exec, spawnRun = spawn } = {}) {
  const configArgs = config ? ['--config', config] : [];
  const clients = new Set();
  let current = null, history = [], sequence = 0, readiness, preview, lastConnect = new Map();
  const hash = crypto.createHash('sha256').update(html.match(/<script>([\s\S]*?)<\/script>/)?.[1] || '').digest('base64');
  const equal = (a,b) => typeof a === 'string' && Buffer.byteLength(a) === Buffer.byteLength(b) && crypto.timingSafeEqual(Buffer.from(a),Buffer.from(b));
  function snapshot() {
    return current ? { id:current.id, task:current.task, mode:current.mode, started:current.started,
      state:current.state, pending:current.pending, summary:current.summary, error:current.error } : null;
  }
  function broadcast(type, data) {
    const event = { seq:++sequence, type, ...data };
    history.push(event); if(history.length > 200) history.shift();
    for(const res of clients) {
      if(res.writableLength > 1024 * 1024) { res.destroy(); clients.delete(res); }
      else res.write(`data: ${JSON.stringify(event)}\n\n`);
    }
  }
  function finish(run, code) {
    if(current !== run) return;
    clearTimeout(run.deadline); clearTimeout(run.killTimer);
    run.pending = null;
    if(run.state === 'stopping') run.state = 'aborted';
    else if(!run.summary) { run.state = 'error'; run.error = run.stderr.trim().slice(-2000) || `Agent exited (${code})`; }
    broadcast('snapshot',{run:snapshot()});
  }
  function stop() {
    if(!current || !['running','waiting','stopping'].includes(current.state)) return;
    const run = current;
    run.state = 'stopping'; run.pending = null;
    run.child.stdin.end();
    run.child.kill('SIGINT');
    if(!run.killTimer) run.killTimer = setTimeout(() => {
      try { process.kill(-run.child.pid,'SIGKILL'); } catch { run.child.kill('SIGKILL'); }
    },3000).unref();
    broadcast('snapshot',{run:snapshot()});
  }
  async function body(req) {
    let data = '', size = 0;
    for await(const chunk of req) {
      size += chunk.length;
      if(size > 16384) throw Object.assign(new Error('Request is too large'),{status:413});
      data += chunk;
    }
    try {return JSON.parse(data);} catch {throw Object.assign(new Error('Invalid request'),{status:400});}
  }
  function json(res,status,data) { res.writeHead(status,{'Content-Type':'application/json'}); res.end(JSON.stringify(data)); }
  function authorized(req) {
    return equal(req.headers.authorization?.replace(/^Bearer /,''),token) ||
      (req.headers.cookie || '').split(';').some(c => equal(c.trim(),`cu_session=${token}`));
  }
  async function status() {
    if(readiness && Date.now()-readiness.at<5000) return {...readiness.value,run:snapshot(),events:history};
    const [native,login] = await Promise.allSettled([
      execute(binary,[...configArgs,'doctor','--json'],{timeout:6000,maxBuffer:16384}),
      execute('codex',['login','status'],{timeout:6000,maxBuffer:16384})
    ]);
    let permissions = {};
    if(native.status==='fulfilled') {try {permissions=JSON.parse(native.value.stdout);} catch {} }
    const loggedIn = login.status==='fulfilled' && /Logged in using ChatGPT/.test(login.value.stdout+login.value.stderr);
    const value = {...permissions,codex:loggedIn,backend:'Native Mac engine',llm:'Codex subscription',
      ready:!!(permissions.screen_recording && permissions.accessibility && loggedIn),
      run:snapshot(),events:history,
      setup:!loggedIn ? "On the Mac, run codex login and sign in with Adam’s ChatGPT account." : !permissions.screen_recording || !permissions.accessibility ? 'On your Mac, open System Settings → Privacy & Security and allow Screen Recording and Accessibility for the app that started cu. Then check the connection again.' : null};
    readiness={at:Date.now(),value}; return {...value,run:snapshot(),events:history};
  }
  async function screenshot() {
    if(preview) return preview;
    preview=(async()=>{
      const dir=await fs.mkdtemp(path.join(os.tmpdir(),'cu-preview-'));
      try {
        const out=path.join(dir,'screen.jpg');
        await execute(binary,[...configArgs,'screenshot','--out',out,'--max','1280'],{timeout:10000,maxBuffer:16384});
        return await fs.readFile(out);
      } finally {await fs.rm(dir,{recursive:true,force:true});}
    })();
    try {return await preview;} finally {preview=null;}
  }
  const server=http.createServer(async(req,res)=>{
    res.setHeader('Cache-Control','no-store');
    res.setHeader('X-Content-Type-Options','nosniff');
    res.setHeader('X-Frame-Options','DENY');
    res.setHeader('Referrer-Policy','no-referrer');
    res.setHeader('Content-Security-Policy',`default-src 'none'; script-src 'sha256-${hash}'; style-src 'unsafe-inline'; img-src 'self' blob:; connect-src 'self'; manifest-src 'self'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'`);
    try {
      const url=new URL(req.url,'http://localhost');
      // Reject cross-site requests, including CSRF against the HttpOnly session cookie.
      const origin=req.headers.origin;
      if(origin && origin!==`http://${req.headers.host}` && origin!==`https://${req.headers.host}`) return json(res,403,{error:'Open this dashboard directly to connect.'});
      if(req.headers['sec-fetch-site']==='cross-site') return json(res,403,{error:'Cross-site access is blocked.'});
      if(req.method==='GET' && url.pathname==='/') {
        res.writeHead(200,{'Content-Type':'text/html; charset=utf-8'}); return res.end(html);
      }
      if(req.method==='GET' && url.pathname==='/manifest.webmanifest') return json(res,200,{name:'cu — Mac control',short_name:'cu',start_url:'/',display:'standalone',background_color:'#f5f4ef',theme_color:'#f5f4ef',icons:[{src:'/icon.svg',sizes:'any',type:'image/svg+xml',purpose:'any'}]});
      if(req.method==='GET' && url.pathname==='/icon.svg') {
        res.writeHead(200,{'Content-Type':'image/svg+xml'}); return res.end('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 192 192"><rect width="192" height="192" rx="48" fill="#273a30"/><text x="96" y="127" text-anchor="middle" font-family="sans-serif" font-size="100" fill="#e2f3a4">cu</text></svg>');
      }
      if(req.method==='POST' && url.pathname==='/api/connect') {
        const key=req.socket.remoteAddress;
        const attempts=lastConnect.get(key) || {at:Date.now(),count:0};
        if(Date.now()-attempts.at>60000) {attempts.at=Date.now();attempts.count=0;}
        attempts.count++; lastConnect.set(key,attempts);
        if(lastConnect.size>1000) lastConnect=new Map([[key,attempts]]);
        if(attempts.count>15) return json(res,429,{error:'Too many attempts. Try again in a minute.'});
        const data=await body(req);
        if(!equal(data.token,token)) return json(res,401,{error:'That connection code is incorrect. Use the link shown on the Mac.'});
        res.setHeader('Set-Cookie',`cu_session=${token}; HttpOnly; SameSite=Strict; Path=/${req.socket.encrypted?'; Secure':''}`);
        return json(res,200,{connected:true});
      }
      if(!authorized(req)) return json(res,401,{error:'Connect using the private link shown on the Mac.'});
      if(req.method==='GET' && url.pathname==='/api/status') return json(res,200,await status());
      if(req.method==='GET' && url.pathname==='/api/events') {
        res.writeHead(200,{'Content-Type':'text/event-stream','Connection':'keep-alive','X-Accel-Buffering':'no'});
        res.write(`data: ${JSON.stringify({type:'connected',run:snapshot(),events:history})}\n\n`);
        clients.add(res);
        const heartbeat=setInterval(()=>res.write(': keepalive\n\n'),15000).unref();
        res.on('close',()=>{clients.delete(res);clearInterval(heartbeat);});
        return;
      }
      if(req.method==='GET' && url.pathname==='/api/screenshot') {
        const bytes=await screenshot(); res.writeHead(200,{'Content-Type':'image/jpeg'}); return res.end(bytes);
      }
      if(req.method==='POST' && url.pathname==='/api/run') {
        if(current && ['running','waiting','stopping'].includes(current.state)) return json(res,409,{error:'A task is already active. Stop it before starting another.'});
        const data=await body(req);
        if(typeof data.task!=='string' || !data.task.trim() || data.task.length>4000) return json(res,400,{error:'Enter a task of up to 4,000 characters.'});
        if(!['confirm-risky','confirm-all','observe'].includes(data.mode)) return json(res,400,{error:'Choose a valid control mode.'});
        const args=[...configArgs,'run',data.task.trim(),'--provider','codex','--mode',data.mode,'--ui'];
        const child=spawnRun(binary,args,{stdio:['pipe','pipe','pipe'],detached:true});
        const run={id:crypto.randomUUID(),task:data.task.trim(),mode:data.mode,started:Date.now(),state:'running',pending:null,child,stderr:''};
        current=run;history=[];
        const lines=createInterface({input:child.stdout});
        lines.on('line',line=>{
          let event;try {event=JSON.parse(line);} catch {return;}
          if(current!==run) return;
          if(event.type==='confirmation') {run.pending=event;run.state='waiting';}
          if(event.type==='done') {run.summary=event.summary;run.pending=null;run.state=event.summary.status;}
          broadcast('agent',{event,run:snapshot()});
        });
        child.stderr.on('data',chunk=>{run.stderr=(run.stderr+chunk).slice(-8000);});
        child.stdin.on('error',()=>{});
        child.once('error',e=>{run.stderr=e.message;finish(run,null);});
        child.once('close',code=>finish(run,code));
        run.deadline=setTimeout(stop,15*60*1000).unref();
        broadcast('snapshot',{run:snapshot()});return json(res,202,{run:snapshot()});
      }
      if(req.method==='POST' && url.pathname==='/api/confirm') {
        const data=await body(req);
        if(!current?.pending || data.runId!==current.id || data.id!==current.pending.id || typeof data.approved!=='boolean') return json(res,409,{error:'That confirmation is no longer pending.'});
        current.child.stdin.write(JSON.stringify({id:data.id,approved:data.approved})+'\n');
        current.pending=null;current.state='running';broadcast('snapshot',{run:snapshot()});return json(res,200,{ok:true});
      }
      if(req.method==='POST' && url.pathname==='/api/stop') {stop();return json(res,200,{run:snapshot()});}
      if(req.method==='POST' && url.pathname==='/api/disconnect') {res.setHeader('Set-Cookie','cu_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0');return json(res,200,{ok:true});}
      json(res,404,{error:'Not found'});
    } catch(error) {if(!res.headersSent) json(res,error.status || 500,{error:error.message});else res.end();}
  });
  server.requestTimeout=15000;server.headersTimeout=10000;
  return { server, token, stop, close:()=>{stop();for(const res of clients)res.end();server.close();} };
}

function start() {
  const dashboard=createDashboard();
  const host=process.env.CU_HOST || '127.0.0.1', port=Number(process.env.CU_PORT || 8787);
  dashboard.server.on('error',error=>{console.error(`Dashboard: ${error.message}`);process.exitCode=1;});
  dashboard.server.listen(port,host,()=>{
    const addresses=host==='0.0.0.0' ? ['127.0.0.1',...Object.values(os.networkInterfaces()).flat().filter(i=>i && i.family==='IPv4' && !i.internal).map(i=>i.address)] : [host.includes(':')?`[${host}]`:host];
    console.log('\ncu — your Mac, within reach\n');
    for(const address of addresses) console.log(`  http://${address}:${dashboard.server.address().port}/#${dashboard.token}`);
    console.log(host==='127.0.0.1' || host==='::1' ? '\nLocal preview on this Mac. To enable phone access, restart with an explicit LAN --host address.\nKeep the private link private. Press Ctrl-C to stop.\n' : '\nOpen a LAN link on your phone on the same trusted Wi-Fi, then add it to your Home Screen.\nThe private link grants access to this Mac. Keep it private. Press Ctrl-C to stop.\n');
  });
  for(const signal of ['SIGINT','SIGTERM'])process.once(signal,()=>dashboard.close());
}
module.exports={createDashboard};
if(!process.env.CU_SERVER_TEST)start();
