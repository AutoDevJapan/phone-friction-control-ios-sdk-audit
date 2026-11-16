//go:build linux && amd64

package main

import (
 "encoding/json"
 "crypto/sha256"
 "io"
 "strconv"
 "bytes"
 "path/filepath"
 "context"
 "runtime"
 "fmt"
 "os"
 "os/exec"
 "syscall"
 "time"
 "sync/atomic"
 "strings"
 "testing"
)
// Data-parser controls only; these never create a qualified native grant.
func TestChildReportRequiresEveryObservedBoundary(t *testing.T){
 n:=strings.Repeat("a",32)
 base:=childResult{Nonce:n,CLIPID:2,RootExit:0,Reaped:1,AllReaped:true}
 valid,_:=json.Marshal(base)
 if _,e:=checkedChildResult(valid,n);e!=nil{t.Fatal(e)}
 for _,fault:=range []string{"nonce","root_failed","unreaped","cancelled","late_writer","error","no_cli","no_wait","missing","truncated","extra","unknown"}{
  t.Run(fault,func(t *testing.T){r:=base
   switch fault{case "nonce":r.Nonce=strings.Repeat("b",32);case "root_failed":r.RootExit=2;case "unreaped":r.AllReaped=false;case "cancelled":r.Cancelled=true;case "late_writer":r.LateWriter=true;case "error":r.Error="wait4";case "no_cli":r.CLIPID=0;case "no_wait":r.Reaped=0}
   raw,_:=json.Marshal(r)
   switch fault{case "missing":raw=[]byte("{}");case "truncated":raw=raw[:len(raw)-2];case "extra":raw=append(raw,[]byte(" {}")...);case "unknown":raw=append(raw[:len(raw)-1],[]byte(",\"Grant\":true}")...)}
   if _,e:=checkedChildResult(raw,n);e==nil{t.Fatal("unobserved report accepted",fault)}
  })
 }
}

// This fixture is never a production mode. The parent test explicitly creates
// the new PID namespace, then requires the exact execution marker and result.
func TestNativeReaperFixtureProcess(t *testing.T){
 if os.Getenv("SDK_NATIVE_REAPER_FIXTURE")!="1"{return}
 if os.Getpid()!=1||os.Geteuid()!=0{os.Exit(70)};if syscall.Mount("","/","",syscall.MS_REC|syscall.MS_PRIVATE,"")!=nil{os.Exit(70)};if syscall.Mount("proc","/proc","proc",syscall.MS_NOSUID|syscall.MS_NODEV|syscall.MS_NOEXEC,"")!=nil{os.Exit(70)}
 r:=childResult{Nonce:"kernel-fixture",RootExit:-1};var timedOut atomic.Bool
 alarm:=time.AfterFunc(4*time.Second,func(){timedOut.Store(true);killNamespace()});defer alarm.Stop()
 script:="exit 0";if os.Getenv("SDK_NATIVE_REAPER_CASE")=="late"{script="sleep 30 </dev/null >/dev/null 2>/dev/null & exit 0"}
 null,e:=os.Open("/dev/null");if e!=nil{os.Exit(70)};defer null.Close()
 pid,e:=syscall.ForkExec("/bin/sh",[]string{"sh","-c",script},&syscall.ProcAttr{Env:[]string{"PATH=/usr/bin:/bin"},Files:[]uintptr{null.Fd(),os.Stderr.Fd(),os.Stderr.Fd()}});if e!=nil{os.Exit(70)}
 r.CLIPID=pid;reapWriterSet(&r,pid);r.Cancelled=timedOut.Load()
 b,_:=json.Marshal(r);fmt.Printf("SDK_NATIVE_REAPER_EXECUTED=%s\n",b);os.Exit(0)
}
func TestNativeKernelReaperNormalAndLateWriter(t *testing.T){
 if e:=rootCapabilities();e!=nil{t.Fatal("required native capability unavailable; not a skip",e)}
 for _,scenario:=range []string{"normal","late"}{t.Run(scenario,func(t *testing.T){
  ctx,cancel:=context.WithTimeout(context.Background(),6*time.Second);defer cancel()
  cmd:=exec.CommandContext(ctx,os.Args[0],"-test.run=^TestNativeReaperFixtureProcess$");cmd.Env=[]string{"PATH=/usr/bin:/bin","SDK_NATIVE_REAPER_FIXTURE=1","SDK_NATIVE_REAPER_CASE="+scenario};cmd.SysProcAttr=&syscall.SysProcAttr{Cloneflags:syscall.CLONE_NEWPID|syscall.CLONE_NEWNS,Pdeathsig:syscall.SIGKILL};cmd.WaitDelay=2*time.Second
  runtime.LockOSThread();defer runtime.UnlockOSThread()
  raw,e:=cmd.CombinedOutput();if e!=nil{t.Fatal("native fixture did not finish",e,string(raw))}
  marker:="SDK_NATIVE_REAPER_EXECUTED=";count:=0;var r childResult
  for _,line:=range strings.Split(string(raw),"\n"){if strings.HasPrefix(line,marker){count++;if e:=json.Unmarshal([]byte(strings.TrimPrefix(line,marker)),&r);e!=nil{t.Fatal(e)}}}
  if count!=1||!r.AllReaped||r.RootExit!=0||r.Cancelled||r.Error!=""{t.Fatal("incomplete kernel execution receipt",r)}
  if scenario=="normal"&&r.LateWriter{t.Fatal("normal fixture falsely refused")};if scenario=="late"&&!r.LateWriter{t.Fatal("live descendant after root close was missed")}
 })}
}

// Real FD inheritance control, prepared only. It is not a qualified-owner grant.
func TestPrivateControlFDIsClosedAtCLIExec(t *testing.T){
 read,write,e:=os.Pipe();if e!=nil{t.Fatal(e)};defer read.Close();defer write.Close()
 fd:=int(write.Fd());_,_,errno:=syscall.Syscall(syscall.SYS_FCNTL,uintptr(fd),syscall.F_SETFD,0);if errno!=0{t.Fatal(errno)}
 run:=func()int{pid,e:=syscall.ForkExec("/bin/sh",[]string{"sh","-c","test ! -e /proc/self/fd/"+fmt.Sprint(fd)},&syscall.ProcAttr{Env:[]string{"PATH=/usr/bin:/bin"},Files:[]uintptr{os.Stdin.Fd(),os.Stdout.Fd(),os.Stderr.Fd()}});if e!=nil{t.Fatal(e)};var status syscall.WaitStatus;if _,e=syscall.Wait4(pid,&status,0,nil);e!=nil{t.Fatal(e)};if !status.Exited(){t.Fatal(status)};return status.ExitStatus()}
 if status:=run();status==0{t.Fatal("unsealed baseline did not expose the control FD")}
 if e=sealControlFD(fd);e!=nil{t.Fatal(e)};if status:=run();status!=0{t.Fatal("private control FD escaped the exec boundary",status)}
}

// Same-image inherited FD controls. No normal constructor accepts caller JSON
// instead of these handles. Tests are prepared for the existing root CI job.
func TestJobCapabilityRequiresActualInheritedObjects(t *testing.T){
 if os.Geteuid()!=0{t.Fatal("root CI capability required; not a skip")}
 dir:=t.TempDir();db,e:=os.Open(dir);if e!=nil{t.Fatal(e)};defer db.Close();image,e:=os.Open("/proc/self/exe");if e!=nil{t.Fatal(e)};defer image.Close();proc,e:=os.Open("/proc");if e!=nil{t.Fatal(e)};defer proc.Close()
 for _,bad:=range []bool{false,true}{hostProc:=proc;if bad{hostProc=db};cmd:=exec.Command(os.Args[0],"-test.run=^TestJobCapabilityFixture$");cmd.Env=[]string{"SDK_JOB_FD_FIXTURE=1","PATH=/usr/bin:/bin"};cmd.ExtraFiles=[]*os.File{db,nil,image,db,image,db,image,db,db,image,nil,nil,hostProc};ctx,cancel:=context.WithTimeout(context.Background(),3*time.Second);owner,e:=startOwned(cmd);if e!=nil{cancel();t.Fatal(e)};e=owner.wait(ctx);cancel();if !owner.observed{owner.stop();t.Fatal("fixture unreaped")};if bad{exit,ok:=e.(*exec.ExitError);if !ok||exit.ExitCode()!=70{t.Fatal("non-proc FD15 was not refused",e)}}else if e!=nil{t.Fatal(e)}}
}
func TestJobCapabilityFixture(t *testing.T){
 if os.Getenv("SDK_JOB_FD_FIXTURE")!="1"{return}
 f,e:=jobCapabilities();if e!=nil{os.Exit(70)};closeJobFiles(f);os.Exit(0)
}

// These are real-kernel controls prepared for the existing root CI invocation.
// They are not exercised by the author's JavaScript model checks.
func TestCopyGraphIncludesEveryOriginalByte(t *testing.T){
 root:=t.TempDir();source:=filepath.Join(root,"source");dest:=filepath.Join(root,"copy");if os.Mkdir(source,0700)!=nil||os.Mkdir(dest,0700)!=nil{t.Fatal("mkdir")};if e:=os.WriteFile(filepath.Join(source,"extra.json"),[]byte("held"),0600);e!=nil{t.Fatal(e)};f,e:=os.Open(source);if e!=nil{t.Fatal(e)};defer f.Close();b:=copyBudget{deadline:time.Now().Add(time.Second)};if e=copyDB(f,dest,"",&b,0);e!=nil{t.Fatal(e)};if len(b.records)!=1||b.records[0].Name!="extra.json"||b.records[0].Bytes!=4{t.Fatal(b.records)};h,e:=copyGraphHash(b.records);if e!=nil||len(h)!=64{t.Fatal(h,e)};bad:=append([]graphMember(nil),b.records...);bad[0].SHA256=strings.Repeat("0",64);other,_:=copyGraphHash(bad);if h==other{t.Fatal("copy byte mutation missed")}
}
func TestWriterFilterRejectsExportAfterExec(t *testing.T){
 if e:=rootCapabilities();e!=nil{t.Fatal("required root capability",e)}
 for _,filtered:=range []bool{false,true}{t.Run(fmt.Sprint(filtered),func(t *testing.T){
 pair,e:=syscall.Socketpair(syscall.AF_UNIX,syscall.SOCK_STREAM,0);if e!=nil{t.Fatal(e)};left:=os.NewFile(uintptr(pair[0]),"writer_socket");right:=os.NewFile(uintptr(pair[1]),"receiver_socket");defer left.Close();defer right.Close();out,e:=os.CreateTemp(t.TempDir(),"output");if e!=nil{t.Fatal(e)};defer out.Close();if _,e=out.Write([]byte("original"));e!=nil{t.Fatal(e)};image,e:=os.Open("/proc/self/exe");if e!=nil{t.Fatal(e)};defer image.Close();readyR,readyW,e:=os.Pipe();if e!=nil{t.Fatal(e)};defer readyR.Close();defer readyW.Close();var received bytes.Buffer
 receiver:=exec.Command("/proc/self/fd/5","-test.run=^TestFDExportFixture$");receiver.Env=[]string{"SDK_FD_EXPORT=receiver"};receiver.ExtraFiles=[]*os.File{right,nil,image,readyR};receiver.Stdout=&received;receiver.Stderr=&received;receiver.SysProcAttr=&syscall.SysProcAttr{Credential:&syscall.Credential{Uid:65533,Gid:65533,Groups:[]uint32{}},Pdeathsig:syscall.SIGKILL}
 runtime.LockOSThread();defer runtime.UnlockOSThread();r,e:=startOwned(receiver);if e!=nil{t.Fatal(e)};defer func(){if !r.observed{r.stop()}}();right.Close();readyR.Close()
 writer:=exec.Command("/proc/self/fd/5","-test.run=^TestFDExportFixture$");writer.Env=[]string{"SDK_FD_EXPORT=writer","SDK_FD_FILTER="+fmt.Sprint(filtered)};writer.ExtraFiles=[]*os.File{left,out,image};writer.Stdout=os.Stderr;writer.Stderr=os.Stderr;writer.SysProcAttr=&syscall.SysProcAttr{Cloneflags:syscall.CLONE_NEWPID|syscall.CLONE_NEWNET,Credential:&syscall.Credential{Uid:65532,Gid:65532,Groups:[]uint32{}},Pdeathsig:syscall.SIGKILL};w,e:=startOwned(writer);if e!=nil{t.Fatal(e)};defer func(){if !w.observed{w.stop()}}();left.Close();ctx,cancel:=context.WithTimeout(context.Background(),6*time.Second);defer cancel();if e=w.wait(ctx);e!=nil||!w.observed{t.Fatal("writer not reaped",e)};if e=out.Chmod(0400);e!=nil{t.Fatal(e)};if _,e=readyW.Write([]byte{1});e!=nil{t.Fatal(e)};readyW.Close();if e=r.wait(ctx);e!=nil||!r.observed{t.Fatal("receiver not reaped",e,received.String())};b,e:=os.ReadFile(out.Name());if e!=nil{t.Fatal(e)};if filtered{if string(b)!="original"||!strings.Contains(received.String(),"FD_EXPORT_BLOCKED"){t.Fatal(string(b),received.String())}}else if string(b)!="escaped!"||!strings.Contains(received.String(),"FD_EXPORT_BASELINE"){t.Fatal("positive transfer not exercised",string(b),received.String())}
 })}
}
func TestFDExportFixture(t *testing.T){
 mode:=os.Getenv("SDK_FD_EXPORT");if mode==""{return}
 if mode=="writer"{if os.Getenv("SDK_FD_FILTER")=="true"{runtime.LockOSThread();_,_,e:=syscall.RawSyscall6(syscall.SYS_PRCTL,prSetNoNewPrivs,1,0,0,0,0);if e!=0||installWriterFilter()!=nil{os.Exit(70)};if e:=syscall.Exec("/proc/self/fd/5",[]string{"test","-test.run=^TestFDExportFixture$"},[]string{"SDK_FD_EXPORT=filtered-after-exec"});e!=nil{os.Exit(70)}};e:=syscall.Sendmsg(3,[]byte{1},syscall.UnixRights(4),nil,0);if e!=nil{os.Exit(70)};os.Exit(0)}
 if mode=="filtered-after-exec"{if e:=syscall.Sendmsg(3,[]byte{1},syscall.UnixRights(4),nil,0);e!=syscall.EPERM{os.Exit(70)};os.Exit(0)}
 if mode=="receiver"{data:=make([]byte,1);oob:=make([]byte,syscall.CmsgSpace(4));n,on,_,_,e:=syscall.Recvmsg(3,data,oob,0);if e!=nil{os.Exit(70)};var ready [1]byte;if _,e=syscall.Read(6,ready[:]);e!=nil{os.Exit(70)};if n==0&&on==0{fmt.Println("FD_EXPORT_BLOCKED");os.Exit(0)};messages,e:=syscall.ParseSocketControlMessage(oob[:on]);if e!=nil||len(messages)!=1{os.Exit(70)};fds,e:=syscall.ParseUnixRights(&messages[0]);if e!=nil||len(fds)!=1{os.Exit(70)};defer syscall.Close(fds[0]);if n,e:=syscall.Pwrite(fds[0],[]byte("escaped!"),0);e!=nil||n!=8{os.Exit(70)};fmt.Println("FD_EXPORT_BASELINE");os.Exit(0)};os.Exit(70)
}

func TestCopyGraphEncodingGolden(t *testing.T){
 records:=[]graphMember{{Name:"z.json",Bytes:0,SHA256:"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"},{Name:"𐀀",Bytes:2,SHA256:"5dde896887f6754c9b15bfe3a441ae4806df2fde94001311e08bf110622e0bbe"},{Name:"",Bytes:3,SHA256:"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"},{Name:"a&.json",Bytes:5,SHA256:"277089d91c0bdf4f2e6862ba7e4a07605119431f5d13f726dd352b06f1b206a9"}}
 got,e:=copyGraphHash(records);if e!=nil||got!="b1693027815ac3f121e840b2cb0b7bb5efeb8b930b4edfaea7fae4bbe1c72609"{t.Fatal("canonical cross-language graph digest",got,e)}
}

func TestCompilerEpochReapsDetachedDescendants(t *testing.T){
 if e:=rootCapabilities();e!=nil{t.Fatal("required root capability",e)};scriptPath,nodePath,want:=os.Getenv("SDK_COMPILER_EPOCH_SCRIPT"),os.Getenv("SDK_COMPILER_EPOCH_NODE"),os.Getenv("SDK_COMPILER_EPOCH_SHA256");script,e:=os.ReadFile(scriptPath);if e!=nil||len(script)==0||fmt.Sprintf("%x",sha256.Sum256(script))!=want{t.Fatal("required pinned compiler epoch program",e)};node,e:=os.Open(nodePath);if e!=nil{t.Fatal(e)};defer node.Close();proc,e:=os.Open("/proc");if e!=nil{t.Fatal(e)};defer proc.Close();parentNS,e:=os.Readlink("/proc/self/ns/pid");if e!=nil{t.Fatal(e)}
 for _,scenario:=range []string{"normal","detached","failed"}{t.Run(scenario,func(t *testing.T){
 reportR,reportW,e:=os.Pipe();if e!=nil{t.Fatal(e)};defer reportR.Close();defer reportW.Close();null,e:=os.Open("/dev/null");if e!=nil{t.Fatal(e)};defer null.Close();shell:="exit 0";wantExit:=0;if scenario!="normal"{shell="sleep 30 </dev/null >/dev/null 2>/dev/null & exit 0"};if scenario=="failed"{shell="sleep 30 </dev/null >/dev/null 2>/dev/null & exit 3";wantExit=3};nonce:=strings.Repeat("a",32);config,_:=json.Marshal(map[string]any{"executable":"/bin/sh","args":[]string{"-c",shell},"env":map[string]string{"PATH":"/usr/bin:/bin"},"cwd":"/","nonce":nonce,"limit":4000})
 cmd:=exec.Command("/usr/bin/unshare","--mount","--pid","--fork","--mount-proc","--kill-child=KILL","/proc/self/fd/7","--input-type=module","--eval",string(script),string(config));cmd.Env=[]string{"PATH=/usr/bin:/bin"};cmd.ExtraFiles=[]*os.File{null,reportW,nil,nil,node};var diagnostic bytes.Buffer;cmd.Stdout=&diagnostic;cmd.Stderr=&diagnostic;cmd.WaitDelay=2*time.Second;runtime.LockOSThread();defer runtime.UnlockOSThread();owner,e:=startOwned(cmd);if e!=nil{t.Fatal(e)};defer func(){if !owner.observed{owner.stop()}}();reportW.Close();ctx,cancel:=context.WithTimeout(context.Background(),6*time.Second);defer cancel();if e=owner.wait(ctx);e!=nil||!owner.observed{t.Fatal("epoch not observed",e,diagnostic.String())};raw,e:=io.ReadAll(io.LimitReader(reportR,4097));if e!=nil||len(raw)>4096{t.Fatal(e)};var r struct{Nonce string;PID,RootExit int;RootObserved bool;RootSignal *string;NamespaceID string};if e=json.Unmarshal(raw,&r);e!=nil||r.Nonce!=nonce||r.PID!=1||!r.RootObserved||r.RootExit!=wantExit||r.RootSignal!=nil||r.NamespaceID==""||r.NamespaceID==parentNS{t.Fatal("wrong epoch observation",string(raw),e)}
 entries,e:=os.ReadDir(anchor(proc));if e!=nil{t.Fatal(e)};for _,entry:=range entries{if _,e:=strconv.Atoi(entry.Name());e!=nil{continue};ns,e:=os.Readlink(filepath.Join(anchor(proc),entry.Name(),"ns/pid"));if os.IsNotExist(e){continue};if e!=nil{t.Fatal("incomplete PID namespace census",e)};if ns==r.NamespaceID{t.Fatal("compiler descendant remained after init Wait",entry.Name())}}
 })}
}
