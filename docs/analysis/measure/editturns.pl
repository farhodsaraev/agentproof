#!/usr/bin/perl
# Share of turns (real user prompts) in which the main agent used a file-edit tool.
use strict; use warnings; use File::Find;
my $root = shift // "$ENV{HOME}/.claude/projects";
my ($turns, $edit_turns) = (0, 0);
my @files;
find(sub { push @files, $File::Find::name if /\.jsonl$/ }, $root);
for my $f (@files) {
    next if $f =~ m{/subagents/};
    my ($proj) = $f =~ m{projects/([^/]+)/};
    next unless $proj && $proj =~ /Desktop-Projects/;
    open my $fh, '<:raw', $f or next;
    my $in_turn = 0; my $edited = 0;
    while (my $l = <$fh>) {
        my $is_prompt = 0;
        if (index($l, '"type":"user"') >= 0 && index($l, '"isMeta":true') < 0 && index($l, '"isSidechain":true') < 0) {
            if ($l =~ /"message":\{"role":"user","content":"((?:[^"\\]|\\.){0,40})/ || $l =~ /"message":\{"role":"user","content":\[\{"type":"text","text":"((?:[^"\\]|\\.){0,40})/) {
                $is_prompt = 1 unless $1 =~ /^(?:<|\[Request interrupted|Caveat:)/;
            }
        }
        if ($is_prompt) {
            if ($in_turn) { $turns++; $edit_turns++ if $edited; }
            $in_turn = 1; $edited = 0; next;
        }
        if ($in_turn && $l =~ /"type":"tool_use","id":"[^"]+","name":"(?:Edit|Write|MultiEdit|NotebookEdit)"/) { $edited = 1; }
    }
    if ($in_turn) { $turns++; $edit_turns++ if $edited; }
}
printf "turns=%d turns_with_file_edits=%d (%.0f%%)\n", $turns, $edit_turns, $turns ? 100*$edit_turns/$turns : 0;
