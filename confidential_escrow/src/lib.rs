use anchor_lang::prelude::*;
use spl_token_2022::extension::confidential_transfer::{
    instruction::inner_transfer
};
use anchor_lang::solana_program::program::{invoke,invoke_signed};
use spl_token_2022::instruction::{ AuthorityType as Atype, set_authority};
use spl_token_2022::state::{Account as TAccount, Mint};
use bytemuck;
use std::{io::{self}, ops::Deref};
use spl_token_2022::solana_zk_token_sdk::{zk_token_elgamal::pod, instruction::transfer::TransferData};

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod confidential_escrow {

    use spl_token_2022::solana_zk_token_sdk::zk_token_proof_instruction::verify_transfer;

    use super::*;
    const ESCROW_PDA_SEED: &[u8] = b"escrow";
    pub fn initialize_escrow(
        ctx: Context<InitializeEscrow>,
        new_source_decryptable_amount: DecryptableBalance,
        taker_amount: EncryptedBalance,
    ) -> Result<()> {
        ctx.accounts.escrow.initializer = *ctx.accounts.initializer.key;
        ctx.accounts.escrow.initializer_receive_account = *ctx.accounts.initializer_receive_account.to_account_info().key;
        ctx.accounts.escrow.initializer_deposit_account = *ctx.accounts.initializer_deposit_token_account.to_account_info().key;
        ctx.accounts.escrow.initializer_decryptable_available_balance = new_source_decryptable_amount;
        ctx.accounts.escrow.taker_amount = taker_amount;

        let (pda, _bump_seed) = Pubkey::find_program_address(&[ESCROW_PDA_SEED], ctx.program_id);
        let ix = set_authority(
            &spl_token_2022::id(),
            &ctx.accounts.initializer_deposit_token_account.to_account_info().key,
            Some(&pda),
            Atype::AccountOwner,
            &ctx.accounts.initializer.key,
            &[],
        )?;
        invoke(
            &ix, 
            &[
                ctx.accounts.initializer_deposit_token_account.to_account_info(),
                ctx.accounts.initializer.to_account_info().clone(),
            ],
        )?;

        Ok(())
    }

    pub fn exchange(
        ctx: Context<Exchange>,
       // proof_instruction_offset: i8,
        taker_proof_instruction_offset: i8,
        transfer_data: Transferdata,
        taker_decryptable_available_balance: DecryptableBalance,
    ) -> Result<()> {
        let (_pda, bump_seed) = Pubkey::find_program_address(&[ESCROW_PDA_SEED], ctx.program_id);
        let seeds = &[&ESCROW_PDA_SEED[..], &[bump_seed]];
        
        // the zk proof has to be generated on the client side and fed to this function
        //the proof is then verified by invoking the verify_transfer fn of 
        //the proof program which is a native program....

        verify_transfer(&transfer_data.0);

        //transferring confidentially from the escrow owned initializer token account to the taker token account
        let ix =  inner_transfer(
            &spl_token_2022::id(),
            &ctx.accounts.pda_deposit_token_account.to_account_info().key,
            &ctx.accounts.taker_deposit_token_account.to_account_info().key,
            &ctx.accounts.initializer_mint.to_account_info().key,
            ctx.accounts.escrow.initializer_decryptable_available_balance.0,
            &_pda,
            &[],
            -1,                            
        )?;
        invoke_signed(
            &ix,
            &[
                ctx.accounts.pda_deposit_token_account.to_account_info(),
                ctx.accounts.taker_receive_token_account.to_account_info(),
                ctx.accounts.initializer_mint.to_account_info(),
            ],
            &[&seeds[..]]
        )?;
        // transferring confidentially from the taker token account to the escrow owned initializer's receiving token account
        let ix_ = inner_transfer(
            &spl_token_2022::id(),
            &ctx.accounts.taker_deposit_token_account.to_account_info().key,
            &ctx.accounts.initializer_receive_account.to_account_info().key,
            &ctx.accounts.taker_mint.to_account_info().key,
            taker_decryptable_available_balance.0,
            &ctx.accounts.taker.key(),
            &[],
            taker_proof_instruction_offset,
        )?;
        invoke(
            &ix_,
            &[
                ctx.accounts.taker_deposit_token_account.to_account_info(),
                ctx.accounts.initializer_receive_account.to_account_info(),
                ctx.accounts.taker_mint.to_account_info(),
            ],
        )?;
        // setting the authority of pda deposit token account back to initializer.
        let set_auth_ix = set_authority(
            &spl_token_2022::id(),
            &ctx.accounts.pda_deposit_token_account.to_account_info().key,
            Some(&ctx.accounts.escrow.initializer),
            Atype::AccountOwner,
            &_pda,
            &[],
        )?;
        invoke(
            &set_auth_ix, 
            &[
                ctx.accounts.pda_deposit_token_account.to_account_info(),
                ctx.accounts.initializer.clone(),
            ],
        )?;
        Ok(())
    }


    pub fn cancel_escrow(ctx: Context<CancelEscrow>) -> Result<()> {
        let (_pda, bump_seed) = Pubkey::find_program_address(&[ESCROW_PDA_SEED], ctx.program_id);
        let seeds = &[&ESCROW_PDA_SEED[..], &[bump_seed]];

        let set_auth_ix = set_authority(
            &spl_token_2022::id(),
            &ctx.accounts.pda_deposit_token_account.to_account_info().key,
            Some(&ctx.accounts.escrow.initializer),
            Atype::AccountOwner,
            &_pda,
            &[],
        )?;
        invoke_signed(
            &set_auth_ix, 
            &[
                ctx.accounts.pda_deposit_token_account.to_account_info(),
                ctx.accounts.initializer.clone(),
            ],
            &[&seeds[..]]

        )?;

        Ok(()) 
    }
}

#[derive(Accounts)]
pub struct InitializeEscrow<'info>{
    #[account(
        init,
        payer = initializer,
        space = 8 + std::mem::size_of::<Escrow>()
    )]
    pub escrow: Box<Account<'info, Escrow>>,
    #[account(mut, constraint = initializer_deposit_token_account.owner == &spl_token_2022::id())]
    pub initializer_deposit_token_account: AccountInfo<'info>,
    #[account(mut, constraint = initializer_receive_account.owner == &spl_token_2022::id())]
    pub initializer_receive_account: AccountInfo<'info>,

    #[account(mut)]
    pub initializer: Signer<'info>,
    #[account(executable, address = spl_token_2022::id())]
    pub token_program: AccountInfo<'info>,
    #[account(executable, address = anchor_lang::system_program::ID)]
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Exchange<'info>{
    pub taker: Signer<'info>,
    #[account(
        address = escrow.initializer
    )]
    pub initializer: AccountInfo<'info>,
    #[account(mut)]
    pub taker_receive_token_account: AccountInfo<'info>,
    #[account(mut)]
    pub taker_deposit_token_account: AccountInfo<'info>,
    #[account(mut)]
    pub initializer_receive_account: AccountInfo<'info>,
    #[account(mut, constraint = pda_deposit_token_account.owner == &pda_account.key())] // the original initializer deposit token account who's authority now is the escrow PDA.
    pub pda_deposit_token_account: AccountInfo<'info>,
    #[account(mut)]
    pub initializer_main_account: AccountInfo<'info>, // the original initializer
    pub initializer_mint: AccountInfo<'info>,
    pub taker_mint: AccountInfo<'info>,
    #[account(
        mut,
        constraint = escrow.initializer_deposit_account == *pda_deposit_token_account.to_account_info().key @ EscrowError::InvalidTokenAccount,
        constraint = escrow.initializer_receive_account == *initializer_receive_account.to_account_info().key @ EscrowError::InvalidTokenAccount,
        constraint = escrow.initializer == *initializer_main_account.key @ EscrowError::InvalidInitializer,
        constraint = escrow.initializer_mint == *initializer_mint.to_account_info().key @ EscrowError::InvalidMint,
        constraint = escrow.taker_mint == *taker_mint.to_account_info().key @ EscrowError::InvalidMint,
        close = initializer_main_account
    )]
    pub escrow: Box<Account<'info, Escrow>>,
    pub pda_account: Signer<'info>,
    #[account(executable, address = spl_token_2022::id())]
    pub token_program: AccountInfo<'info>,
}

#[derive(Accounts)]
pub struct CancelEscrow<'info>{
    pub initializer: AccountInfo<'info>,
    #[account(mut)]
    pub pda_deposit_token_account: AccountInfo<'info>,
    pub pda_account: AccountInfo<'info>,
    #[account(
        mut,
        constraint = escrow.initializer == *initializer.key,
        constraint = escrow.initializer_deposit_account == *pda_deposit_token_account.to_account_info().key,
        close = initializer
    )]
    pub escrow: Box<Account<'info, Escrow>>,
    #[account(executable, address = spl_token_2022::id())]
    pub token_program: AccountInfo<'info>,
}





#[derive(Clone, Copy, Debug)]
pub struct EncryptedBalance(pod::ElGamalCiphertext);

#[derive(Clone, Copy, Debug)]
pub struct DecryptableBalance(pod::AeCiphertext);


#[derive(Clone, Copy, Debug)]
pub struct ElGamalKey(pod::ElGamalPubkey);

#[derive(Clone, Copy)]
pub struct Transferdata(TransferData);
#[account]
pub struct Escrow {
    pub initializer: Pubkey,
    pub initializer_mint: Pubkey,
    pub taker_mint: Pubkey,
    pub initializer_deposit_account: Pubkey,
    pub initializer_receive_account: Pubkey,
    //pub initializer_amount: EncryptedBalance,
    pub initializer_decryptable_available_balance: DecryptableBalance,
    pub taker_amount: EncryptedBalance,
}
#[error_code]
pub enum EscrowError {
    #[msg("invalid encrypted amount entered")]
    InvalidAmount,
    #[msg("You're passing an Invalid token account")]
    InvalidTokenAccount,
    #[msg("Invalid initializer passed in...")]
    InvalidInitializer,
    #[msg("Invalid mint...")]
    InvalidMint,

}






impl AnchorSerialize for EncryptedBalance {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> io::Result<()>{
        let buf = bytemuck::bytes_of(&self.0);
        w.write_all(buf)?;
        Ok(())
    }
}
impl AnchorDeserialize for EncryptedBalance{
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        let cipher = *bytemuck::try_from_bytes(buf).unwrap();
        Ok(Self(cipher))
    }
}
/* 
impl AnchorSerialize for ElGamalKey {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> io::Result<()>{
        let buf = bytemuck::bytes_of(&self.0);
        w.write_all(buf)?;
        Ok(())
    }
}
impl AnchorDeserialize for ElGamalKey{
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        let key = *bytemuck::try_from_bytes(buf).unwrap();
        Ok(Self(key))
    }
}*/



impl AnchorSerialize for DecryptableBalance {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> io::Result<()>{
        let buf = bytemuck::bytes_of(&self.0);
        w.write_all(buf)?;
        Ok(())
    }
}
impl AnchorDeserialize for DecryptableBalance{
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        let cipher = *bytemuck::try_from_bytes(buf).unwrap();
        Ok(Self(cipher))
    }
}
impl Deref for DecryptableBalance{
    type Target = pod::AeCiphertext;
    fn deref(&self) -> &Self::Target{
        &self.0
    }
}

impl AnchorSerialize for Transferdata {
    fn serialize<W: std::io::Write>(&self, w: &mut W) -> io::Result<()>{
        let buf = bytemuck::bytes_of(&self.0);
        w.write_all(buf)?;
        Ok(())
    }
}
impl AnchorDeserialize for Transferdata{
    fn deserialize(buf: &mut &[u8]) -> io::Result<Self> {
        let data = *bytemuck::try_from_bytes(buf).unwrap();
        Ok(Self(data))
    }
}
