#!/usr/bin/perl
# One measurement of piped test commands over all local Claude Code transcripts.
# Definitions:
#   test command = a Bash tool call in which one pipeline stage starts with a
#                  known test runner (after env assignments and wrappers).
#   piped        = that stage is not the last stage of its pipeline (output goes
#                  into "|"), and the command has no "pipefail" or "PIPESTATUS".
# Each tool call is counted once (dedupe by tool_use id across files).
use strict; use warnings;
use File::Find;
use JSON::PP;

my $root = shift // "$ENV{HOME}/.claude/projects";
my $js = JSON::PP->new->allow_nonref;
my (%seen, %proj_total, %proj_piped, %sess, %target, %runner);
my ($total, $piped, $pf, $bash_calls) = (0, 0, 0, 0);
my ($mind, $maxd) = ('9999', '0000');

sub strip_heredocs {
    my ($c) = @_;
    my @lines = split /\n/, $c, -1;
    my @out; my $end;
    for my $l (@lines) {
        if (defined $end) { if ($l =~ /^\s*\Q$end\E\s*$/) { undef $end; } next; }
        push @out, $l;
        if ($l =~ /<<-?\s*['"]?([A-Za-z_][A-Za-z0-9_]*)['"]?/) { $end = $1; }
    }
    return join "\n", @out;
}

sub norm {
    my ($c) = @_;
    $c = strip_heredocs($c);
    # Inline bash -c "..." and sh -c '...'
    $c =~ s/\b(?:bash|sh|zsh)(?:\s+-[a-z]+)*\s+-[a-z]*c\s+"((?:[^"\\]|\\.)*)"/ ( $1 ) /g;
    $c =~ s/\b(?:bash|sh|zsh)(?:\s+-[a-z]+)*\s+-[a-z]*c\s+'([^']*)'/ ( $1 ) /g;
    # Remove quoted strings
    $c =~ s/'[^']*'/''/g;
    $c =~ s/"(?:[^"\\]|\\.)*"/""/g;
    # Protect redirections that contain & or |
    $c =~ s/\d?>&\d/ /g; $c =~ s/&>>?/ /g; $c =~ s/>\|/>/g;
    return $c;
}

my $RUN = qr{
   ^(?:pytest|py\.test)\b
 | ^(?:python3?|py)(?:\.exe)?(?:\s+-\S+)*\s+-m\s+(?:pytest|unittest|nose2?)\b
 | ^(?:python3?|py)(?:\.exe)?\s+\S*manage\.py\s+test\b
 | ^cargo(?:\s+\+\S+)?(?:\s+-\S+)*\s+(?:test|nextest)\b
 | ^go\s+test\b
 | ^dotnet\s+test\b
 | ^(?:\./)?(?:mvnw?|gradlew?)\b.*\s(?:test|verify|check)\b
 | ^make\b.*\s(?:test|check)\b
 | ^(?:\./)?(?:node_modules/\.bin/)?(?:jest|vitest|mocha|ava|rspec|phpunit|tox|nox|ctest|bats)\b
 | ^playwright\s+test\b
 | ^deno\s+test\b
 | ^mix\s+test\b
 | ^nx\s+(?:test\b|run\s+\S+:test\b|run-many\b.*test|affected\b.*test)
}x;

sub runner_stage {
    my ($s) = @_;
    $s =~ s/^[\s({!]+//;
    for (1..6) {
        my $o = $s;
        $s =~ s/^(?:[A-Za-z_][A-Za-z0-9_]*=\S*\s+)+//;
        $s =~ s/^(?:time|exec|env|nice|stdbuf\s+\S+|timeout(?:\s+-\S+)*\s+\S+)\s+//;
        $s =~ s/^(?:npx|bunx|pnpm\s+(?:exec|dlx)|yarn\s+(?:exec|dlx)|uv\s+run|poetry\s+run|pipenv\s+run|hatch\s+run)(?:\s+-\S+)*\s+//;
        last if $s eq $o;
    }
    return 'runner' if $s =~ $RUN;
    if ($s =~ /^(?:npm|pnpm|yarn|bun)\b/ && $s !~ /^\S+\s+(?:i|install|add|ci|remove|uninstall)\b/) {
        return 'pkg-test' if $s =~ /\s(?:run\s+)?(?:test|t)(?::[\w.:-]+)?(?:\s|$)/ || $s =~ /\srun\s+[\w.-]+:test[\w.:-]*(?:\s|$)/;
    }
    if ($s =~ /^docker(?:-compose)?\b/) {
        return 'docker' if $s =~ /\s(?:cargo\s+(?:test|nextest)|pytest|go\s+test|npm\s+(?:run\s+)?test|pnpm\s+(?:run\s+)?test|yarn\s+test|bun\s+test|jest|vitest|dotnet\s+test)\b/;
    }
    return '';
}

sub analyze {
    my ($cmd) = @_;
    my $c = norm($cmd);
    my $is_test = 0; my $is_piped = 0; my $tgt = '';
    for my $pl (split /\|\||&&|;|\n|&/, $c) {
        my @st = split /\|&?/, $pl;
        for my $i (0..$#st) {
            my $r = runner_stage($st[$i]);
            next unless $r;
            $is_test = 1; $runner{$r}++;
            if ($i < $#st) {
                $is_piped = 1;
                ($tgt) = $st[$i+1] =~ /^\s*(\S+)/; $tgt //= '?';
            }
        }
    }
    my $has_pf = ($cmd =~ /pipefail|PIPESTATUS/) ? 1 : 0;
    return ($is_test, $is_piped, $has_pf, $tgt);
}

my @files;
find(sub { push @files, $File::Find::name if /\.jsonl$/ }, $root);
for my $f (@files) {
    my ($proj) = $f =~ m{projects/([^/]+)/};
    $proj //= '?';
    open my $fh, '<:raw', $f or next;
    while (my $line = <$fh>) {
        next unless index($line, '"name":"Bash"') >= 0;
        my ($date) = $line =~ /"timestamp":"(\d{4}-\d\d-\d\d)/;
        my ($sid) = $line =~ /"sessionId":"([^"]+)"/;
        while ($line =~ /"id":"(toolu_[^"]+)","name":"Bash","input":\{"command":"((?:[^"\\]|\\.)*)"/g) {
            my ($id, $raw) = ($1, $2);
            next if $seen{$id}++;
            $bash_calls++;
            my $cmd = eval { $js->decode("\"$raw\"") } // next;
            my ($t, $p, $hpf, $tgt) = analyze($cmd);
            next unless $t;
            $total++; $proj_total{$proj}++;
            $sess{$sid // $f} = 1;
            if (defined $date) { $mind = $date if $date lt $mind; $maxd = $date if $date gt $maxd; }
            if ($p && !$hpf) { $piped++; $proj_piped{$proj}++; $target{$tgt}++; }
            $pf++ if $p && $hpf;
        }
    }
    close $fh;
}
printf "files=%d bash_calls=%d test_cmds=%d sessions=%d dates=%s..%s\n", scalar(@files), $bash_calls, $total, scalar(keys %sess), $mind, $maxd;
printf "piped(no pipefail)=%d (%.1f%%) piped_with_pipefail=%d\n", $piped, $total ? 100*$piped/$total : 0, $pf;
print "by project:\n";
for my $p (sort { $proj_total{$b} <=> $proj_total{$a} } keys %proj_total) {
    my $pp = $proj_piped{$p} // 0;
    printf "  %-90s %5d piped %5d (%.0f%%)\n", substr($p,0,90), $proj_total{$p}, $pp, 100*$pp/$proj_total{$p};
}
print "pipe targets: ", join(', ', map { "$_=$target{$_}" } (sort { $target{$b} <=> $target{$a} } keys %target)[0..9]), "\n";
print "runner kinds: ", join(', ', map { "$_=$runner{$_}" } sort keys %runner), "\n";
