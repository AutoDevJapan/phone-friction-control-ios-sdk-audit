//go:build linux && amd64

package main

import (
 "bufio"
 "context"
 "crypto/rand"
 "encoding/hex"
 "encoding/json"
 "fmt"
 "errors"
 "io"
 "os"
 "os/exec"
 "runtime"
 "strings"
 "syscall"
 "time"
)
type jobFiles struct{ db,image,diagnostics,node,source,build,stage,temp,official,hostProc *os.File }
func inheritedFile(fd int,kind uint32)(*os.File,error){var s syscall.Stat_t;if e:=syscall.Fstat(fd,&s);e!=nil||s.Mode&syscall.S_IFMT!=kind{return nil,fail("job_inherited_object_unavailable")};f:=os.NewFile(uintptr(fd),"job_object");if f==nil{return nil,fail("job_inherited_object_unavailable")};return f,nil}
func jobCapabilities()(jobFiles,error){
 var f jobFiles;var e error
 if os.Geteuid()!=0{return f,fail("job_root_unavailable")}
 // Validate all roles before opening any new resource. Literal numbers alone
 // do not issue these objects: the trusted root build is their actual creator.
 // This local call-graph premise does not authenticate an external issuer against host root.
 roles:=[]struct{fd int;kind uint32;target **os.File}{{3,syscall.S_IFDIR,&f.db},{5,syscall.S_IFREG,&f.image},{6,syscall.S_IFDIR,&f.diagnostics},{7,syscall.S_IFREG,&f.node},{8,syscall.S_IFDIR,&f.source},{9,syscall.S_IFREG,&f.build},{10,syscall.S_IFDIR,&f.stage},{11,syscall.S_IFDIR,&f.temp},{12,syscall.S_IFREG,&f.official},{15,syscall.S_IFDIR,&f.hostProc}}
 for _,r:=range roles{if *r.target,e=inheritedFile(r.fd,r.kind);e!=nil{return f,e}}
 var procFS syscall.Statfs_t;if syscall.Fstatfs(15,&procFS)!=nil||procFS.Type!=0x9fa0{return f,fail("job_host_proc_capability")};for _,v:=range []*os.File{f.hostProc,f.image,f.diagnostics,f.node,f.source,f.build,f.stage,f.official}{s,e:=stat(v);if e!=nil||s.Uid!=0||s.Mode&022!=0{return f,fail("job_root_object_mutable")}}
 a,e:=os.Open("/proc/self/exe");if e!=nil{return f,e};defer a.Close();actual,e:=stat(a);if e!=nil{return f,e};held,e:=stat(f.image);if e!=nil||!same(actual,held){return f,fail("job_image_object_changed")}
 return f,nil
}
func jobExtra(f jobFiles,control *os.File)[]*os.File{return []*os.File{f.db,control,f.image,f.diagnostics,f.node,f.source,f.build,f.stage,f.temp,f.official,nil,nil,f.hostProc}}
func closeJobFiles(f jobFiles){for _,v:=range []*os.File{f.db,f.image,f.diagnostics,f.node,f.source,f.build,f.stage,f.temp,f.official,f.hostProc}{if v!=nil{v.Close()}}}
func safeDiagnosticExport(f jobFiles)error{
 owner,e:=stat(f.temp);if e!=nil{return e};allowed:=map[string]bool{}
 for _,name:=range []string{"actual-default-setup.json","rust.sarif","verified-inputs.json","canonical-rust.sarif","verified-database.json","verified-flight.json","verification-failure.json","producer-failure.json","producer-stderr.txt","sarif-upload-result.json","database-upload-result.json","consumer-failure.json","bootstrap-build.json","native-owner-provisional.json","native-owner-final.json"}{allowed[name]=true}
 entries,e:=os.ReadDir(anchor(f.diagnostics));if e!=nil{return e}
 for _,entry:=range entries{if !allowed[entry.Name()]{return fail("job_diagnostic_member")};p:=anchor(f.diagnostics)+"/"+entry.Name();out,e:=nofollow(p,syscall.O_RDONLY|syscall.O_NONBLOCK);if e!=nil{return e};s,e:=stat(out);if e!=nil||!regular(s)||s.Nlink!=1||s.Uid!=0{out.Close();return fail("job_diagnostic_object")};e=out.Chown(int(owner.Uid),int(owner.Gid));if e==nil{e=out.Chmod(0400)};out.Close();if e!=nil{return e}}
 if e=f.diagnostics.Chown(int(owner.Uid),int(owner.Gid));e!=nil{return e};return f.diagnostics.Chmod(0700)
}
type jobInitResult struct { Nonce string; NodeExit int; AllReaped,LateChild,Finished bool }
func jobFlight()(result error){
 f,e:=jobCapabilities();if e!=nil{return e};defer closeJobFiles(f)
 started,reaped:=false,false
 defer func(){
  final:=struct{Primary string;AllReaped,DiagnosticsExported,StageRemoved bool}{AllReaped:reaped}
  if result!=nil{final.Primary=result.Error()}
  if !started||reaped{
   if e:=safeDiagnosticExport(f);e!=nil{result=errors.Join(result,e)}else{final.DiagnosticsExported=true}
   stage,e:=os.Readlink(anchor(f.stage));if e==nil&&strings.HasPrefix(stage,"/tmp/codeql-root-job."){
    held,se:=stat(f.stage);i,ie:=os.Lstat(stage)
    if se==nil&&ie==nil&&same(held,i.Sys().(*syscall.Stat_t)){if e=os.RemoveAll(stage);e==nil{_,e=os.Lstat(stage);if os.IsNotExist(e){final.StageRemoved=true}else{result=errors.Join(result,fail("job_stage_not_removed"))}}else{result=errors.Join(result,e)}}else{result=errors.Join(result,fail("job_stage_identity"))}
   }else{result=errors.Join(result,fail("job_stage_reference"))}
  }else{result=errors.Join(result,fail("job_cleanup_writers_unverified"))}
  b,_:=json.Marshal(final);fmt.Fprintf(os.Stderr,"root_job_final=%s\n",b)
 }()
 if e=rootCapabilities();e!=nil{return e}
 build,e:=io.ReadAll(io.LimitReader(f.build,1<<20+1));if e!=nil||len(build)>1<<20{return fail("job_build_receipt")}
 diag,e:=os.OpenFile(anchor(f.diagnostics)+"/bootstrap-build.json",os.O_WRONLY|os.O_CREATE|os.O_EXCL,0400);if e!=nil{return e};_,we:=diag.Write(build);ce:=diag.Close();if we!=nil{return we};if ce!=nil{return ce}
 var nonce [16]byte;if _,e=rand.Read(nonce[:]);e!=nil{return e};n:=hex.EncodeToString(nonce[:]);reportR,reportW,e:=os.Pipe();if e!=nil{return e};defer reportR.Close();cfgR,cfgW,e:=os.Pipe();if e!=nil{reportW.Close();return e}
 runtime.LockOSThread();defer runtime.UnlockOSThread()
 cmd:=exec.Command("/proc/self/fd/5","--internal-job-init");cmd.ExtraFiles=append(jobExtra(f,nil),reportW,cfgR);cmd.Stdin=os.Stdin;cmd.Stdout=os.Stdout;cmd.Stderr=os.Stderr;cmd.Env=os.Environ();cmd.SysProcAttr=&syscall.SysProcAttr{Cloneflags:syscall.CLONE_NEWPID|syscall.CLONE_NEWNS,Pdeathsig:syscall.SIGKILL}
 child,e:=startOwned(cmd);reportW.Close();cfgR.Close();if e!=nil{cfgW.Close();return fail("job_pid_namespace_unavailable")};started=true
 if _,e=cfgW.Write([]byte(n+"\n"));e!=nil{cfgW.Close();child.stop();return fail("job_namespace_control")};cfgW.Close()
 ctx,cancel:=context.WithTimeout(context.Background(),20*time.Minute);defer cancel();e=child.wait(ctx);if !child.observed{child.stop();return fail("job_lifetime_unverified")}
 raw,readErr:=io.ReadAll(io.LimitReader(reportR,4097));if readErr!=nil||len(raw)>4096{return fail("job_namespace_report")}
 var r jobInitResult;decoder:=json.NewDecoder(strings.NewReader(string(raw)));decoder.DisallowUnknownFields();if decoder.Decode(&r)!=nil||decoder.Decode(new(any))!=io.EOF||r.Nonce!=n{return fail("job_namespace_report")}
 reaped=r.AllReaped
 if e!=nil||!r.AllReaped||r.NodeExit!=0||r.LateChild||!r.Finished{return fail("job_root_flight_failed")};return nil
}
type jobMessage struct{ Kind,Nonce,Token string; Publication bool; Reason string }
func jobInit()int{
 if os.Getpid()!=1{return 70};if rootCapabilities()!=nil{return 70};if syscall.Mount("","/","",syscall.MS_REC|syscall.MS_PRIVATE,"")!=nil{return 70};if syscall.Mount("proc","/proc","proc",syscall.MS_NOSUID|syscall.MS_NODEV|syscall.MS_NOEXEC,"")!=nil{return 70};f,e:=jobCapabilities();if e!=nil{return 70};defer closeJobFiles(f)
 report:=os.NewFile(16,"job_result");config:=os.NewFile(17,"job_nonce");if report==nil||config==nil{return 70};defer report.Close();defer config.Close();if sealControlFD(16)!=nil||sealControlFD(17)!=nil{return 70}
 nraw,e:=io.ReadAll(io.LimitReader(config,65));if e!=nil||len(nraw)!=33||nraw[32]!='\n'{return 70};n:=string(nraw[:32]);if _,e=hex.DecodeString(n);e!=nil{return 70}
 result:=jobInitResult{Nonce:n,NodeExit:-1}
 finish:=func(code int)int{b,_:=json.Marshal(result);if len(b)>1024{return 70};if _,e=report.Write(append(b,'\n'));e!=nil{return 70};return code}
 inputR,inputW,e:=os.Pipe();if e!=nil{return finish(70)};controlR,controlW,e:=os.Pipe();if e!=nil{inputR.Close();inputW.Close();return finish(70)};defer inputW.Close();defer controlR.Close()
 files:=[]uintptr{inputR.Fd(),controlW.Fd(),os.Stderr.Fd()};for _,file:=range jobExtra(f,nil){if file==nil{files=append(files,^uintptr(0))}else{files=append(files,file.Fd())}}
 env:=[]string{};for _,v:=range os.Environ(){key,_,_:=strings.Cut(v,"=");upper:=strings.ToUpper(key);if strings.Contains(upper,"TOKEN")||strings.Contains(upper,"SECRET")||strings.Contains(upper,"PASSWORD")||strings.Contains(upper,"CREDENTIAL"){continue};env=append(env,v)};env=append(env,"SDK_ROOT_JOB=1")
 runtime.LockOSThread();pid,e:=syscall.ForkExec("/proc/self/fd/7",[]string{"node","--input-type=module","--eval","import('node:url').then(u=>import(u.pathToFileURL('/proc/self/fd/8/codeql-root-entry.mjs').href))"},&syscall.ProcAttr{Env:env,Files:files});runtime.UnlockOSThread();inputR.Close();controlW.Close();if e!=nil{return finish(70)}
 initial,_:=json.Marshal(jobMessage{Kind:"bootstrap",Nonce:n});if _,e=inputW.Write(append(initial,'\n'));e!=nil{killNamespace();return finish(70)}
 done:=make(chan childResult,1);go func(){r:=childResult{RootExit:-1,CLIPID:pid};reapWriterSet(&r,pid);done<-r}()
 type message struct{origin string;body jobMessage};messages:=make(chan message);bad:=make(chan string,2)
 readMessages:=func(reader io.Reader,origin string){scan:=bufio.NewScanner(reader);scan.Buffer(make([]byte,4096),32768);for scan.Scan(){var m jobMessage;d:=json.NewDecoder(strings.NewReader(scan.Text()));d.DisallowUnknownFields();if d.Decode(&m)!=nil||d.Decode(new(any))!=io.EOF{bad<-"invalid-"+origin;return};messages<-message{origin,m}};if scan.Err()!=nil{bad<-"invalid-"+origin;return};bad<-origin}
 go readMessages(controlR,"node");go readMessages(os.Stdin,"parent")
 timer:=time.NewTimer(20*time.Minute);defer timer.Stop();prepared,tokenSent,finished,nodeEOF:=false,false,false,false;var nodeDone *childResult
 setResult:=func(r childResult){result.NodeExit=r.RootExit;result.AllReaped=r.AllReaped;result.LateChild=r.LateWriter;result.Finished=finished}
 stop:=func()int{killNamespace();if nodeDone!=nil{setResult(*nodeDone);return finish(70)};select{case r:=<-done:setResult(r);return finish(70);case <-time.After(2*time.Second):return finish(70)}}
 for{if nodeDone!=nil&&nodeEOF{r:=*nodeDone;setResult(r);if !finished||r.RootExit!=0||!r.AllReaped||r.LateWriter||r.Error!=""{return finish(70)};return finish(0)};select{
 case received:=<-messages:
  m:=received.body;if m.Nonce!=n{return stop()}
  switch m.Kind{
  case "prepared":if received.origin!="node"||prepared||finished{return stop()};prepared=true;m.Token="";b,_:=json.Marshal(m);if _,e=os.Stdout.Write(append(b,'\n'));e!=nil{return stop()}
  case "token":if nodeDone!=nil{return stop()};if received.origin!="parent"||!prepared||tokenSent||finished||m.Token==""||len(m.Token)>16384{return stop()};tokenSent=true;b,_:=json.Marshal(m);if _,e=inputW.Write(append(b,'\n'));e!=nil{return stop()};m.Token=""
  case "finished":if received.origin!="node"||!prepared||!tokenSent||finished{return stop()};finished=true;m.Token="";b,_:=json.Marshal(m);if _,e=os.Stdout.Write(append(b,'\n'));e!=nil{return stop()}
  default:return stop()
  }
 case origin:=<-bad:if origin=="node"{nodeEOF=true}else{return stop()}
 case r:=<-done:nodeDone=&r
 case <-timer.C:return stop()
 }}
}
