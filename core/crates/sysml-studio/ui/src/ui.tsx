import { Badge, Box, Flex, Separator, Text } from '@radix-ui/themes';
import type { CanvasIssue } from './types';

/**
 * The inspector's three building blocks.
 *
 * They are here rather than in `Inspector.tsx` because every panel uses them, and a section that
 * looked different in one panel would read as a different kind of thing.
 */

export function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <Box px="3" py="2">
      <Separator size="4" mb="2" />
      <Text as="p" size="1" weight="medium" color="gray" style={{ textTransform: 'uppercase', letterSpacing: '0.04em' }}>
        {title}
      </Text>
      <Box mt="2">{children}</Box>
    </Box>
  );
}

export function KV({ name, value, mono }: { name: string; value: string; mono?: boolean }) {
  return (
    <Flex gap="2" align="baseline" py="1">
      {name && <Text size="1" color="gray" style={{ minWidth: 92, flexShrink: 0 }}>{name}</Text>}
      <Text size="1" style={{ fontFamily: mono ? 'ui-monospace, Consolas, monospace' : undefined, wordBreak: 'break-word' }}>
        {value}
      </Text>
    </Flex>
  );
}

export function IssueBox({ issue }: { issue: CanvasIssue }) {
  return (
    <Flex align="start" gap="2" py="1">
      <Badge size="1" variant="soft" color={issue.severity === 'error' ? 'red' : issue.severity === 'warning' ? 'amber' : 'gray'}>
        {issue.code}
      </Badge>
      <Text size="1">{issue.message}</Text>
    </Flex>
  );
}
