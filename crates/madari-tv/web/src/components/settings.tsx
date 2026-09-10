import {Card} from '@astryxdesign/core/Card';
import {Divider} from '@astryxdesign/core/Divider';
import {HStack} from '@astryxdesign/core/HStack';
import {Icon} from '@astryxdesign/core/Icon';
import {Stack, StackItem} from '@astryxdesign/core/Stack';
import {Text} from '@astryxdesign/core/Text';
import {VStack} from '@astryxdesign/core/VStack';
import * as stylex from '@stylexjs/stylex';
import {Children, type ComponentType, type ReactNode, type SVGProps} from 'react';

/** One width for every control on the surface, so the control column reads as a line. */
export const CONTROL_WIDTH = 208;

/** The panel column rows measure themselves against. */
const PANEL_CONTAINER = 'settings-panel';

/**
 * One width at which every row on a panel goes from two columns to one. Rows
 * that break together read as the layout changing; rows that break separately
 * read as a bug.
 */
const ROW_STACK = `@container ${PANEL_CONTAINER} (max-width: 520px)`;

/** The one inset for this surface: spacing step 4. */
const ROW_PADDING = 4;

export type RowIcon = ComponentType<SVGProps<SVGSVGElement>>;

const styles = stylex.create({
  panel: {
    containerType: 'inline-size',
    containerName: PANEL_CONTAINER,
  },
  line: {
    display: 'flex',
    flexDirection: {default: 'row', [ROW_STACK]: 'column'},
    alignItems: {default: 'center', [ROW_STACK]: 'stretch'},
    gap: {
      default: 'var(--spacing-4)',
      [ROW_STACK]: 'var(--spacing-2)',
    },
  },
  textColumn: {
    maxWidth: {default: 460, [ROW_STACK]: 'none'},
  },
  control: {
    flexShrink: 0,
  },
});

/** The column every row asks its width question of. */
export function SettingsPanel({children}: {children: ReactNode}) {
  return (
    <VStack gap={4} xstyle={styles.panel}>
      {children}
    </VStack>
  );
}

/**
 * A filled, divided group of settings rows. Cards group one subject, and the
 * rows carry the inset so the dividers stay full-bleed.
 */
export function SettingsCard({title, children}: {title?: string; children: ReactNode}) {
  const rows = Children.toArray(children);
  return (
    <VStack gap={1.5}>
      {title != null ? (
        <Text type="supporting" weight="semibold" color="secondary">
          {title}
        </Text>
      ) : null}
      <Card padding={0} width="100%" variant="muted">
        <VStack as="ul" role="list" gap={0}>
          {rows.map((row, index) => (
            <VStack key={index} as="li" gap={0}>
              {index > 0 ? <Divider variant="subtle" /> : null}
              {row}
            </VStack>
          ))}
        </VStack>
      </Card>
    </VStack>
  );
}

/**
 * One setting: name and explanation on the left, the control on the right — or,
 * below the panel's stacking width, the control underneath at full width.
 *
 * Pass controls `isLabelHidden`: this row is their visible label.
 */
export function SettingsRow({
  title,
  titleAccessory,
  description,
  icon,
  control,
  detail,
}: {
  title: string;
  titleAccessory?: ReactNode;
  description?: ReactNode;
  icon: RowIcon;
  control?: ReactNode;
  /** Content that cannot fit beside the setting: a preview, a picker, an ordered list. */
  detail?: ReactNode;
}) {
  return (
    <VStack padding={ROW_PADDING} gap={2}>
      <Stack xstyle={styles.line}>
        <StackItem size="fill">
          <HStack gap={2} align="start">
            <VStack paddingBlockStart={0.5}>
              <Icon icon={icon} size="sm" color="secondary" />
            </VStack>
            <VStack gap={0.5} xstyle={styles.textColumn}>
              {titleAccessory == null ? (
                <Text type="label">{title}</Text>
              ) : (
                <HStack gap={1.5} align="center">
                  <Text type="label">{title}</Text>
                  {titleAccessory}
                </HStack>
              )}
              {description != null ? (
                <Text type="supporting" color="secondary">
                  {description}
                </Text>
              ) : null}
            </VStack>
          </HStack>
        </StackItem>
        {control != null ? (
          <VStack xstyle={styles.control}>{control}</VStack>
        ) : null}
      </Stack>
      {detail}
    </VStack>
  );
}
