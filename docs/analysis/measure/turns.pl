#!/usr/bin/perl
# Count real user prompts (turns) per session and per day in main transcripts.
use strict; use warnings; use File::Find;
my $root = shift // "$ENV{HOME}/.claude/projects";
my (%per_day, %per_sess, %day_proj);
my @files;
find(sub { push @files, $File::Find::name if /\.jsonl$/ }, $root);
for my $f (@files) {
    next if $f =~ m{/subagents/};
    my ($proj) = $f =~ m{projects/([^/]+)/};
    next unless $proj && $proj =~ /Desktop-Projects/;
    my ($sid) = $f =~ m{([0-9a-f-]{36})\.jsonl$};
    open my $fh, '<:raw', $f or next;
    while (my $l = <$fh>) {
        next unless index($l, '"type":"user"') >= 0;
        next if index($l, '"isMeta":true') >= 0;
        next if index($l, '"isSidechain":true') >= 0;
        next if index($l, '"tool_result"') >= 0 && $l !~ /"content":"/;
        my $ok = 0;
        if ($l =~ /"message":\{"role":"user","content":"((?:[^"\\]|\\.){0,40})/) {
            my $c = $1;
            $ok = 1 unless $c =~ /^(?:<|\[Request interrupted|Caveat:)/;
        } elsif ($l =~ /"message":\{"role":"user","content":\[\{"type":"text","text":"((?:[^"\\]|\\.){0,40})/) {
            my $c = $1;
            $ok = 1 unless $c =~ /^(?:<|\[Request interrupted|Caveat:)/;
        }
        next unless $ok;
        my ($d) = $l =~ /"timestamp":"(\d{4}-\d\d-\d\d)/;
        $d //= '?';
        $per_day{$d}++; $per_sess{"$proj/$sid"}++; $day_proj{$d}{$proj}++;
    }
}
my @days = sort keys %per_day;
my @counts = sort { $a <=> $b } map { $per_day{$_} } @days;
my $sum = 0; $sum += $_ for @counts;
printf "active days=%d total prompts=%d median/day=%s max/day=%d mean/day=%.1f\n",
  scalar(@days), $sum, @counts ? $counts[int(@counts/2)] : 0, @counts ? $counts[-1] : 0, @days ? $sum/@days : 0;
for my $d (@days) { printf "  %s %4d  %s\n", $d, $per_day{$d}, join(' ', map { (my $p=$_) =~ s/.*Projects-//; "$p=$day_proj{$d}{$_}" } sort keys %{$day_proj{$d}}); }
print "sessions:\n";
for my $s (sort { $per_sess{$b} <=> $per_sess{$a} } keys %per_sess) { (my $x=$s) =~ s/.*Projects-//; printf "  %-70s %d\n", $x, $per_sess{$s}; }
