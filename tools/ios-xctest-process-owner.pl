#!/usr/bin/env perl
use strict;
use warnings;
use POSIX qw(setsid WNOHANG);
use Fcntl qw(F_GETFL F_SETFL O_NONBLOCK);
use Time::HiRes qw(time sleep);
use IO::Handle;

my $out = shift @ARGV // die "missing output directory\n";
@ARGV or die "missing XCTest command\n";
my $status_path = "$out/captured-selection-context-xctest-owner-status.txt";
my $stop_path = "$out/captured-selection-context-xctest-stop.txt";
die "stale stop request\n" if -e $stop_path;
pipe(my $read, my $write) or die "pipe: $!\n";
pipe(my $ack_read, my $ack_write) or die "ack pipe: $!\n";
fcntl($read, F_SETFL, fcntl($read, F_GETFL, 0) | O_NONBLOCK)
    or die "nonblocking pipe: $!\n";
my $started = time;
$SIG{PIPE} = 'IGNORE';
my $leader = fork();
defined $leader or die "fork: $!\n";
if (!$leader) {
    close $read;
    close $ack_write;
    setsid() >= 0 or die "setsid: $!\n";
    $SIG{TERM} = 'IGNORE';
    print {$write} "group=$$\n";
    $write->flush();
    my $ack = <$ack_read>;
    die "missing group acknowledgement\n" unless defined $ack && $ack eq "go\n";
    close $ack_read;
    my $command = fork();
    defined $command or die "command fork: $!\n";
    if (!$command) {
        close $write;
        $SIG{TERM} = 'DEFAULT';
        $SIG{INT} = 'DEFAULT';
        exec @ARGV;
        die "exec: $!\n";
    }
    waitpid($command, 0) == $command or die "command wait: $!\n";
    my $code = ($? & 127) ? 128 + ($? & 127) : $? >> 8;
    print {$write} "exit=$code\n";
    $write->flush();
    close $write;
    # Keep this group leader alive until the owner terminates the entire group.
    # The parent has not reaped us, so neither PID nor PGID can be reused.
    while (1) { sleep 0.1; }
}
close $write;
close $ack_read;
my $requested = 0;
$SIG{TERM} = sub { $requested = 143; };
$SIG{INT} = sub { $requested = 130; };
$SIG{HUP} = sub { $requested = 129; };
my ($group_ready, $code, $buffer, $reason) = (0, undef, '', 'command');
while (!defined $code) {
    my $chunk = '';
    my $count = sysread($read, $chunk, 4096);
    if (defined $count && $count > 0) {
        $buffer .= $chunk;
        while ($buffer =~ s/^([^\n]*)\n//) {
            my $line = $1;
            if ($line eq "group=$leader" && !$group_ready && !$requested && !-e $stop_path) {
                $group_ready = 1;
                print {$ack_write} "go\n";
                $ack_write->flush();
            }
            elsif ($line =~ /^exit=([0-9]+)$/ && $group_ready) { $code = 0 + $1; }
            else { $code = 1; $reason = 'invalid-owner-message'; }
        }
    } elsif (defined $count && $count == 0) {
        $code = 1; $reason = 'owner-pipe-closed';
    }
    if ($requested || -e $stop_path) {
        $code = $requested || 143; $reason = 'stop-request';
    } elsif (time - $started >= 180) {
        $code = 124; $reason = 'process-deadline';
    }
    sleep 0.02 unless defined $code;
}
close $read;
close $ack_write;
# No waitpid has run for leader yet. A zombie leader still pins its numeric ID.
my $signal_target = $group_ready ? -$leader : $leader;
kill 'TERM', $signal_target;
sleep 1;
kill 'KILL', $signal_target;
my $reaped = 0;
my $reap_started = time;
while (time - $reap_started < 2) {
    my $got = waitpid($leader, WNOHANG);
    if ($got == $leader) { $reaped = 1; last; }
    last if $got < 0;
    sleep 0.02;
}
my $remaining = 0;
my $query_verified = 1;
if ($group_ready) {
    # Read-only observation after reap; never signal a retired numeric ID.
    my $snapshot = '';
    my $ok = eval {
        local $SIG{ALRM} = sub { die "ps deadline\n" };
        alarm 5;
        open(my $ps, '-|', 'ps', '-axo', 'pgid=') or die "ps: $!\n";
        local $/;
        $snapshot = <$ps> // '';
        close $ps or die "ps status\n";
        alarm 0;
        1;
    };
    alarm 0;
    $query_verified = $ok ? 1 : 0;
    $remaining = $ok ? (scalar(grep { /^\s*\Q$leader\E\s*$/ } split /\n/, $snapshot) ? 1 : 0) : 'unknown';
}
open(my $status, '>', $status_path) or die "status: $!\n";
print {$status} "xctest_owner_reason=$reason\ncommand_status=$code\nleader_group_established=$group_ready\nleader_reaped=$reaped\nxctest_group_query_verified=$query_verified\nxctest_process_group_remaining=$remaining\nretired_numeric_ids_signalled=false\n";
close $status or die "status close: $!\n";
exit(($reaped && $query_verified && $remaining eq '0') ? $code : 1);
