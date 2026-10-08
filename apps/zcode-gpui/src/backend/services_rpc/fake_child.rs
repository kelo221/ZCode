//! Synthetic pipe peer, compiled only for tests. No services, Host or storage.
use super::{ServiceClient, ServiceSpawnError, client::StartupTimeouts};
use std::path::PathBuf;
use std::time::Duration;

pub(crate) fn spawn(mode: &str) -> Result<ServiceClient, ServiceSpawnError> {
    spawn_with_timeouts(mode, Duration::from_secs(10))
}

pub(super) fn spawn_with_timeouts(
    mode: &str,
    timeout: Duration,
) -> Result<ServiceClient, ServiceSpawnError> {
    spawn_configured(mode, timeout, None)
}

pub(super) fn spawn_tree(
    mode: &str,
    root: &std::path::Path,
) -> Result<ServiceClient, ServiceSpawnError> {
    spawn_configured(mode, Duration::from_secs(3), Some(root))
}

fn spawn_configured(
    mode: &str,
    timeout: Duration,
    root: Option<&std::path::Path>,
) -> Result<ServiceClient, ServiceSpawnError> {
    let program = std::env::var_os("ZCODE_GPUI_TEST_NODE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(windows) {
                PathBuf::from("C:/Users/V/AppData/Local/Programs/ZCode/ZCode.exe")
            } else {
                PathBuf::from("node")
            }
        });
    let args = vec!["--eval".into(), PEER.into(), mode.into()];
    let mut envs = vec![("ELECTRON_RUN_AS_NODE".into(), "1".into())];
    if let Ok(system_root) = std::env::var("SystemRoot") {
        envs.push(("SystemRoot".into(), system_root));
    }
    if let Some(root) = root {
        envs.push((
            "ZCODE_TREE_SCRATCH".into(),
            root.to_string_lossy().into_owned(),
        ));
        envs.push((
            "ZCODE_TREE_EXE".into(),
            std::env::current_exe()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        ));
    }
    ServiceClient::spawn_with_timeouts(
        &program,
        &args,
        &envs,
        &std::env::temp_dir(),
        StartupTimeouts {
            hello: timeout,
            initialize: timeout,
        },
    )
}

const PEER: &str = r#"
(function() {
const mode = process.argv[1];
const timer = setInterval(() => {}, 1000);
if (process.env.ZCODE_TREE_EXE) {
  const fs=require('fs'), path=require('path');
  const leaf=require('child_process').spawn(process.env.ZCODE_TREE_EXE,
    ['--ignored','--exact','backend::services_rpc::process_tree_tests::owned_tree_fixture','--nocapture','--quiet'],
    {env:{...process.env,ZCODE_TREE_FIXTURE:'leaf'},stdio:['ignore','pipe',1]});
  // 后代 stderr 持有 root stdout；单等 root 无法得到 EOF，必须先核验整个子树。
  leaf.stdout.on('data',()=>{});
  while(!fs.existsSync(path.join(process.env.ZCODE_TREE_SCRATCH,'leaf-ready'))) {
    Atomics.wait(new Int32Array(new SharedArrayBuffer(4)),0,0,5);
  }
}
if (mode === 'silent') return;
if (mode === 'bad_hello') {
  process.stdout.write('{"type":"zcode-hello","pid":"wrong"}\n');
  return;
}
process.stdout.write(JSON.stringify({type:'zcode-hello',version:'fixture',platform:'test',arch:'test',pid:process.pid})+'\n');
let input = Buffer.alloc(0), acked = false, pair, cancels = 0;
function vql(n) {
  const bytes=[];
  do { const b=n&127; n=n>>>7; bytes.push(b|(n?128:0)); } while(n);
  return Buffer.from(bytes);
}
function enc(value) {
  if (value === undefined) return Buffer.from([0]);
  if (typeof value === 'string') {
    const b=Buffer.from(value); return Buffer.concat([Buffer.from([1]),vql(b.length),b]);
  }
  if (Array.isArray(value)) return Buffer.concat([Buffer.from([4]),vql(value.length),...value.map(enc)]);
  if (typeof value === 'number' && (value|0)===value) return Buffer.concat([Buffer.from([6]),vql(value)]);
  const b=Buffer.from(JSON.stringify(value)); return Buffer.concat([Buffer.from([5]),vql(b.length),b]);
}
function decode(b) {
  let pos=0;
  function n() { let x=0; for(let s=0;;s+=7) { const c=b[pos++];x|=(c&127)<<s;if(!(c&128))return x; } }
  function value() {
    const tag=b[pos++];
    if(tag===0)return undefined;
    if(tag===6)return n();
    if(tag===4)return Array.from({length:n()},value);
    const len=n(), bytes=b.subarray(pos,pos+len);pos+=len;
    if(tag===1)return bytes.toString();
    if(tag===5)return JSON.parse(bytes.toString());
    throw new Error('fixture request tag');
  }
  return [value(),value()];
}
function packet(header, value) {
  const b=Buffer.concat([enc(header),enc(value)]), h=Buffer.alloc(13);
  h[0]=1;h.writeUInt32BE(b.length,9);return Buffer.concat([h,b]);
}
function reply(id,value,kind=201) { process.stdout.write(packet([kind,id],value)); }
function request(header,args) {
  const [kind,id,channel,method]=header;
  if(kind===101) { cancels++;reply(id,'late cancelled result');return; }
  if(kind!==100||channel!=='test'||!Array.isArray(args))process.exit(33);
  switch(method) {
    case 'pair':
      if(!pair)pair=[id,args[0]];
      else { const bytes=Buffer.concat([packet([201,id],args[0]),packet([201,pair[0]],pair[1])]);process.stdout.write(bytes);pair=null; }
      break;
    case 'echo': reply(id,args[0]);break;
    case 'void': reply(id,undefined);break;
    case 'null': reply(id,null);break;
    case 'hold': break;
    case 'cancel_count': reply(id,cancels);break;
    case 'error': case 'error_obj': reply(id,{message:'synthetic-secret',name:'Error',stack:['synthetic-secret']},method==='error'?202:203);break;
    case 'exit': process.exit(0);break;
    case 'reply_exit': process.stdout.write(packet([201,id],'drained'),()=>process.exit(0));break;
    case 'future_id': reply(id+100,'uncorrelated');break;
    case 'oversize': { const h=Buffer.alloc(13);h[0]=1;h.writeUInt32BE(2*1024*1024+1,9);process.stdout.write(h);break; }
    case 'bad_frame': process.stdout.write(packet([999,id],undefined));break;
    default: process.exit(34);
  }
}
process.stdin.on('data',chunk=> {
  input=Buffer.concat([input,chunk]);
  if(!acked) {
    const newline=input.indexOf(10);if(newline<0)return;
    const ack=JSON.parse(input.subarray(0,newline).toString());
    if(ack.type!=='zcode-hello-ack'||typeof ack.version!=='string'||!ack.clientId)process.exit(31);
    input=input.subarray(newline+1);acked=true;
    if(mode==='no_initialize')return;
    if(mode==='bad_initialize')process.stdout.write(packet([200],null));
    else if(mode==='truncated') { process.stdout.write(Buffer.from([1,0]));process.exit(0); }
    else { const b=packet([200],undefined);for(const byte of b)process.stdout.write(Buffer.from([byte])); }
  }
  while(input.length>=13) {
    const len=input.readUInt32BE(9);if(input.length<13+len)return;
    const [header,args]=decode(input.subarray(13,13+len));input=input.subarray(13+len);request(header,args);
  }
});
process.stdin.on('end',()=>process.exit(0));
})();
"#;
