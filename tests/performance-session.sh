set -euo pipefail
source "$here/tests/session.bash"
"$Z" quit
sleep .2
python3 - "$Z" "$OUT" <<'PY'
import os,sys,time,socket,subprocess,json,statistics
from pathlib import Path
z,out=sys.argv[1:];sock=Path(os.environ['XDG_RUNTIME_DIR'])/'zl.sock'
def ipc(cmd):
 s=socket.socket(socket.AF_UNIX);s.settimeout(1);s.connect(str(sock));s.sendall(cmd.encode());s.shutdown(socket.SHUT_WR);b=s.recv(256);s.close();assert b.startswith(b'ok'),b
stats=[]
for i in range(9):
 t=time.perf_counter();p=subprocess.Popen([z],stdout=subprocess.DEVNULL,stderr=open(Path(out)/f'daemon-{i}.log','w'))
 while True:
  try: ipc('hide');break
  except (OSError,AssertionError):
   assert p.poll() is None; time.sleep(.001)
 ready=(time.perf_counter()-t)*1e3
 t=time.perf_counter();ipc('show');show=(time.perf_counter()-t)*1e3
 time.sleep(.15);ipc('hide');t=time.perf_counter();ipc('show');warm=(time.perf_counter()-t)*1e3
 stats.append([ready,show,warm]);ipc('quit');p.wait()
p=subprocess.Popen([z],stdout=subprocess.DEVNULL,stderr=open(Path(out)/'idle.log','w'))
while True:
 try: ipc('hide');break
 except OSError: time.sleep(.005)
def proc():
 vals=Path(f'/proc/{p.pid}/stat').read_text().split();rss=int(Path(f'/proc/{p.pid}/status').read_text().split('VmRSS:')[1].split()[0]);return int(vals[13])+int(vals[14]),rss
a=proc();t=time.perf_counter();time.sleep(3);b=proc();elapsed=time.perf_counter()-t
hidden={'rss_kib':b[1],'cpu_percent':(b[0]-a[0])/os.sysconf('SC_CLK_TCK')/elapsed*100}
ipc('show');time.sleep(.3);a=proc();t=time.perf_counter();time.sleep(3);b=proc();elapsed=time.perf_counter()-t
visible={'rss_kib':b[1],'cpu_percent':(b[0]-a[0])/os.sysconf('SC_CLK_TCK')/elapsed*100}
subprocess.run(['grim',str(Path(out)/'visible.png')],check=True);ipc('quit');p.wait()
r={'method':'9 fresh processes, warm filesystem; startup through ready hide ack; show through IPC ack, not presented frame','samples_ms':stats,'median_ms':dict(zip(['startup_ready','first_show','warm_show'],map(statistics.median,zip(*stats)))),'hidden_idle_3s':hidden,'visible_idle_3s':visible,'binary_bytes':Path(z).stat().st_size}
Path(out,'performance.json').write_text(json.dumps(r,indent=2));print(json.dumps(r,indent=2))
PY
