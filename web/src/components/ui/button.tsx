// shadcn/ui's Button (Radix Slot + cva), restyled with the section 5 tokens. Every target is at least 44 px.
import { Slot } from '@radix-ui/react-slot';
import { cva, type VariantProps } from 'class-variance-authority';
import type { ComponentProps } from 'react';
import { cn } from '@/lib/cn';

// eslint-disable-next-line react-refresh/only-export-components
export const buttonVariants = cva(
  'inline-flex min-h-11 items-center justify-center gap-2 rounded-[10px] px-4 text-[15px] font-medium transition-colors disabled:cursor-not-allowed',
  {
    variants: {
      variant: {
        primary: 'bg-teal text-on-teal font-semibold disabled:bg-status-drawn-bg disabled:text-graphite',
        secondary: 'border border-control-line bg-surface text-ink disabled:text-graphite',
        ghost: 'bg-transparent text-graphite hover:text-ink',
      },
      size: {
        md: 'h-11',
        lg: 'h-[54px] rounded-xl text-base',
        icon: 'h-11 w-11 px-0',
      },
    },
    defaultVariants: { variant: 'secondary', size: 'md' },
  },
);

export interface ButtonProps extends ComponentProps<'button'>, VariantProps<typeof buttonVariants> {
  asChild?: boolean;
}

export function Button({ className, variant, size, asChild = false, type, ...props }: ButtonProps) {
  const Comp = asChild ? Slot : 'button';
  return (
    <Comp
      className={cn(buttonVariants({ variant, size }), className)}
      type={asChild ? undefined : (type ?? 'button')}
      {...props}
    />
  );
}
