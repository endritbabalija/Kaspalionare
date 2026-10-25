// shadcn/ui's DropdownMenu over Radix (focus, keyboard and screen-reader handling), restyled with the tokens.
import * as M from '@radix-ui/react-dropdown-menu';
import type { ComponentProps } from 'react';
import { cn } from '@/lib/cn';

export const DropdownMenu = M.Root;
export const DropdownMenuTrigger = M.Trigger;
export const DropdownMenuGroup = M.Group;
export const DropdownMenuRadioGroup = M.RadioGroup;

export function DropdownMenuContent({ className, sideOffset = 6, ...props }: ComponentProps<typeof M.Content>) {
  return (
    <M.Portal>
      <M.Content
        sideOffset={sideOffset}
        className={cn('z-50 min-w-56 rounded-xl border border-rule bg-surface p-1 text-ink shadow-lg', className)}
        {...props}
      />
    </M.Portal>
  );
}

const item =
  'flex min-h-11 cursor-pointer select-none items-center gap-2 rounded-lg px-3 text-[15px] outline-none data-[highlighted]:bg-rule-soft data-[disabled]:cursor-not-allowed data-[disabled]:text-graphite';

export function DropdownMenuItem({ className, ...props }: ComponentProps<typeof M.Item>) {
  return <M.Item className={cn(item, className)} {...props} />;
}

export function DropdownMenuRadioItem({ className, children, ...props }: ComponentProps<typeof M.RadioItem>) {
  return (
    <M.RadioItem className={cn(item, 'justify-between', className)} {...props}>
      {children}
      <M.ItemIndicator>
        <svg
          width="16"
          height="16"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          aria-hidden="true"
        >
          <path d="M5 12l5 5 9-10" />
        </svg>
      </M.ItemIndicator>
    </M.RadioItem>
  );
}

export function DropdownMenuLabel({ className, ...props }: ComponentProps<typeof M.Label>) {
  return <M.Label className={cn('px-3 pt-2 pb-1 text-[13px] text-graphite', className)} {...props} />;
}

export function DropdownMenuSeparator({ className, ...props }: ComponentProps<typeof M.Separator>) {
  return <M.Separator className={cn('my-1 h-px bg-rule-soft', className)} {...props} />;
}
