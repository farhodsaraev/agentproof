#!/usr/bin/perl
# Estimate the size of the AI request (engine/src/ai.rs build()) for saved checks.
# It uses the same limits as the engine: prompt 2,000 chars, last 60 commands,
# command 500 chars, error 300 chars, file body 20,000 chars, all bodies 120,000 chars.
# Diff bodies are estimated: (added + deleted lines) x (average line length of the
# current file + 7 chars for the "+123: " prefix and the newline).
use strict; use warnings; use JSON::PP;
my $SYSTEM = 1060;   # characters in the SYSTEM text
my $SCHEMA = 700;    # characters of the JSON schema (structured output)
my @rows;
for my $f (@ARGV) {
    open my $fh, '<:raw', $f or next; local $/; my $txt = <$fh>; close $fh;
    my $d = eval { JSON::PP->new->utf8->decode($txt) } or next;
    my $s = $d->{session} or next;
    my $root = $d->{start_point}{repo_root} // $d->{project} // '';
    $root =~ s{\\}{/}g; $root =~ s{^([A-Za-z]):}{"/".lc($1)}e;
    my ($p, $fm, $c, $df, $nfiles) = (0, 0, 0, 0, 0);
    for my $pr (@{ $s->{prompts} // [] }) {
        my $t = $pr->{text} // ''; my $n = length($t); $n = 2000 + 12 if $n > 2000;
        $p += $n + 25;
    }
    $fm = length($s->{final_message} // '') + 60;
    my @cmds = @{ $s->{commands} // [] };
    my $skip = @cmds > 60 ? @cmds - 60 : 0;
    for my $cm (@cmds[$skip .. $#cmds]) {
        my $n = length($cm->{command} // ''); $n = 512 if $n > 500;
        $c += $n + 45;
        if (defined $cm->{error}) { my $e = length($cm->{error}); $e = 312 if $e > 300; $c += $e + 4; }
    }
    my $total = 0;
    for my $file (@{ $d->{changes}{files} // [] }) {
        next if ($file->{origin} // '') =~ /before/i;
        $nfiles++;
        my $lines = ($file->{added} // 0) + ($file->{deleted} // 0);
        my $avg = 40;
        my $path = "$root/$file->{path}";
        if (-f $path && -s $path) {
            my $size = -s $path;
            if (open my $in, '<:raw', $path) { my $nl = 0; while (<$in>) { $nl++ } close $in; $avg = $size / ($nl || 1) if $nl; }
        }
        $avg = 200 if $avg > 200;
        my $body = int($lines * ($avg + 7));
        $body = 20000 + 12 if $body > 20000;
        $total += $body;
        last if $total > 120000;
        $df += $body + length($file->{path}) + 10;
    }
    my $chars = $SYSTEM + $SCHEMA + $p + $fm + $c + $df + 200;
    (my $name = $f) =~ s{.*/(C--[^/]+)/([0-9a-f]{8})[^/]*/check\.json$}{$1/$2};
    $name =~ s/.*Projects-//;
    push @rows, [$name, scalar(@{ $s->{prompts} // [] }), scalar(@cmds), $nfiles, $p, $fm, $c, $df, $chars];
}
printf "%-45s %7s %5s %5s %8s %7s %8s %8s %9s %8s\n", qw(check prompts cmds files promptCh finalCh cmdCh diffCh totalCh ~tokens);
for my $r (sort { $a->[8] <=> $b->[8] } @rows) {
    printf "%-45s %7d %5d %5d %8d %7d %8d %8d %9d %8d\n", @$r, $r->[8] / 3;
}
